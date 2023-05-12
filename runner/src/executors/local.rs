use super::ExecutorError;
use crate::{
    collector::TestCollector,
    config::{ExecutorConfig, SolverConfig},
    database::{ConnectionAdapters, TestMetrics},
    ingest::{IngestorMap, RunOutput},
};
use affinity::{get_core_num, set_thread_affinity};
use cowstr::CowStr;
use itertools::iproduct;
use rayon::{prelude::*, ThreadPoolBuilder};
use std::{
    ffi::OsStr,
    io::Read,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tracing::{debug, error, info, instrument, span, warn, Level};
use tracing_unwrap::{OptionExt, ResultExt};
use wait_timeout::ChildExt;

const ATOMIC_ORDERING: Ordering = Ordering::SeqCst;

/// Executor that works on a local rayon-backed thread pool
#[derive(Debug)]
pub struct LocalExecutor<'a> {
    config: SolverConfig,
    connection: ConnectionAdapters,
    ingestors: IngestorMap<'a>,
    collectors: Vec<TestCollector>,
}

impl<'a> LocalExecutor<'a> {
    /// create a new LocalExecutor instance
    pub fn load(
        connection: ConnectionAdapters,
        config: SolverConfig,
        ingestors: IngestorMap<'a>,
        collectors: Vec<TestCollector>,
    ) -> Result<Self, ExecutorError> {
        Ok(Self {
            connection,
            config,
            ingestors,
            collectors,
        })
    }

    /// execute jobs concurrently with a thread pool
    #[instrument(skip(self), level = "info")]
    pub fn execute(self) -> Result<(), ExecutorError> {
        // setup custom global thread pool
        match self.config.executor {
            ExecutorConfig::Local {
                mut threads,
                pinned,
            } => {
                if threads == 0 {
                    warn!("0 threads for thread pool are not possible, falling back to number of CPUs");

                    threads = get_core_num();
                }

                let mut builder = ThreadPoolBuilder::new();

                if pinned {
                    info!("Pinning threads to logical CPUs");

                    // TODO: Add config option for fine grained pinning control, this is a late
                    // stage feature

                    // cores are spread over all threads, this is done by pinning threads to CPU from high ->
                    // low via CPU affinity
                    let free_cores = AtomicUsize::new(threads - 1);
                    builder = builder.start_handler(move |thread_handle| {
                        let selected_core = free_cores.fetch_sub(1, ATOMIC_ORDERING);

                        debug!("Pinning thread-pool thread {thread_handle} to logical CPU {selected_core}");
                        set_thread_affinity([selected_core]).expect("Failed to pin thread to CPU");
                    });
                }

                builder.num_threads(threads).build_global().unwrap_or_log();

                debug!("Building thread pool with {threads} threads");
            }

            #[cfg(feature = "distributed")]
            ExecutorConfig::Distributed { .. } => unreachable!(),
        }

        // general counters to provide a progress bar
        let total = AtomicU64::new(0);
        let processed = AtomicU64::new(0);
        let total_iterations = AtomicU64::new(0);
        let errors = AtomicU64::new(0);

        // find all files
        self.config
            .tests
            .iter()
            .zip(self.collectors.into_iter())
            // ensure set.solvers is always defined
            // Wrap Data in clonable, thread-safe types
            .map(|((name, set), collector)| {
                (CowStr::from(name.as_str()), Arc::from(set), collector)
            })
            .flat_map(|(name, set, collector)| {
                // collect list of paths by recursively searching with glob filtering
                // TODO: add better error handling below
                let paths = collector.iter().unwrap_or_log();
                debug!("Found paths: {paths:?}");

                // increase total counter for progress bar
                total.fetch_add((paths.len() * set.solvers.len()) as u64, ATOMIC_ORDERING);

                // create actual tasks for all sets x solvers, including test metadata for
                // ingesting
                iproduct!(
                    paths,
                    set.solvers
                        .iter()
                        .map(|solver| CowStr::from(solver.as_str()))
                )
                .map(move |(path, solver)| (name.clone(), set.clone(), solver, path))
            })
            .par_bridge()
            .for_each(|(name, set, solver_name, file)| {
                let span = span!(
                    Level::INFO,
                    "threadpool-execution",
                    testset = %name,
                    solver = %solver_name,
                    file = %file.to_string_lossy()
                );
                let _enter = span.enter();

                debug!(
                    "Processing {:?} with {solver_name:?} for {name} with timeout {}",
                    file, set.timeout
                );

                // TODO: Another map type may be used here to allow for fast access
                // For testing this is sufficient though
                let solver = self.config.solvers.get(&solver_name).unwrap_or_log();
                let timeout = Duration::from_millis(set.timeout as u64);

                for iteration in 0..set.iterations {
                    let span = span!(
                        Level::INFO,
                        "threadpool-execution-iteration",
                        iteration = iteration
                    );
                    let _enter = span.enter();

                    let start = Instant::now();

                    // this thread is created after the initial thread and inherits it's CPU affinity
                    match Command::new(&solver.exec)
                        .args(solver.params.iter().map(|solver| solver.as_str()))
                        .args(set.params.iter().map(|solver| solver.as_str()))
                        .arg(file.as_os_str())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                    {
                        Ok(mut child) => match child.wait_timeout(timeout).unwrap_or_log() {
                            Some(status) => {
                                let mut output = RunOutput::new();
                                output.status = status.code().unwrap_or(i32::MIN);

                                // TODO: Add a lot of error fallback around this, in particular the
                                // unwrap and expect parts that involve io
                                output.runtime = start.elapsed().as_millis();
                                let mut stdout = child.stdout.take().unwrap_or_log();
                                stdout
                                    .read_to_string(&mut output.stdout)
                                    .expect("Failed to read stdout");

                                let mut stderr = child.stderr.take().unwrap_or_log();
                                stderr
                                    .read_to_string(&mut output.stderr)
                                    .expect("Failed to read stderr");

                                debug!(
                                    test = %name,
                                    solver = %solver_name,
                                    "Finished in {} ms | status: {}",
                                    output.runtime,
                                    status
                                );

                                match self.ingestors.get(&solver.ingest).unwrap().ingest(output) {
                                    Ok(metrics) => {
                                        debug!("Inserting {metrics:?}...");

                                        match self.connection.store(
                                            metrics,
                                            solver_name.clone(),
                                            name.clone(),
                                            &file.to_path_buf(),
                                        ) {
                                            Ok(id) => {
                                                debug!("Saved run {}", id);
                                            }
                                            Err(e) => {
                                                error!(error = ?e, "Failed to insert metric: {e}");
                                                errors.fetch_add(1, ATOMIC_ORDERING);
                                            }
                                        };
                                    }
                                    Err(e) => {
                                        error!(
                                            solver = %solver_name,
                                            set = %name,
                                            file = %file.to_string_lossy(),
                                            error = ?e,
                                            "Failed to ingest record: {e}"
                                        );

                                        errors.fetch_add(1, ATOMIC_ORDERING);
                                    }
                                };
                            }
                            None => {
                                // child hasn't exited yet
                                child.kill().unwrap_or_log();

                                debug!(
                                    set = %name,
                                    solver = %solver_name,
                                    file = %file.to_string_lossy(),
                                    "Killed due to timeout"
                                );

                                // NOTE: This is guaranteed by the timeout in the config being limited
                                // in size
                                match self.connection.store(
                                    TestMetrics::failed(),
                                    solver_name.clone(),
                                    name.clone(),
                                    &file.to_path_buf(),
                                ) {
                                    Ok(id) => {
                                        debug!(id = id, "Saved run {id}");
                                    }
                                    Err(e) => {
                                        error!(error = ?e, "Failed to insert metric: {e}");
                                        errors.fetch_add(1, ATOMIC_ORDERING);
                                    }
                                };
                            }
                        },
                        Err(e) => {
                            warn!(error = ?e, "Failed to spawn child process: {e}");

                            errors.fetch_add(1, ATOMIC_ORDERING);
                        }
                    };

                    info!(
                        "Done with [{iteration}/{}] - [{name}/{solver_name}/{}] {}/{} [errors: {}]",
                        set.iterations,
                        file.file_name()
                            .unwrap_or(OsStr::new("?"))
                            .to_string_lossy(),
                        processed.load(ATOMIC_ORDERING),
                        total.load(ATOMIC_ORDERING),
                        errors.load(ATOMIC_ORDERING)
                    );

                    total_iterations.fetch_add(1, ATOMIC_ORDERING);
                }

                processed.fetch_add(1, ATOMIC_ORDERING);
            });

        self.connection.close()?;

        // finish the whole thing with a small confirmation message
        info!(
            "Done with processing {} items and a total of {} executions",
            total.load(ATOMIC_ORDERING),
            total_iterations.load(ATOMIC_ORDERING),
        );

        let encountered_errors = errors.load(ATOMIC_ORDERING);

        if encountered_errors > 0 {
            warn!("{encountered_errors} errors were encountered during execution, consult the logs for more information")
        }

        Ok(())
    }
}
