use super::ExecutorError;
use crate::{
    config::SolverConfig,
    database::{util::IDMap, Connection},
    ingest::{IngestorError, IngestorMap, RunContext, RunOutput},
};
use affinity::{get_core_num, set_thread_affinity};
use cowstr::CowStr;
use duckdb::params;
use ignore::WalkBuilder;
use itertools::{iproduct, Itertools};
use rayon::{prelude::*, ThreadPoolBuilder};
use std::{
    borrow::Cow,
    ffi::OsStr,
    io::Read,
    process::{exit, Command, Stdio},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tracing::{debug, error, info, instrument, warn};
use wait_timeout::ChildExt;

const ATOMIC_ORDERING: Ordering = Ordering::SeqCst;

/// Executor that works on a local rayon-backed thread pool
#[derive(Debug, Clone)]
pub struct LocalExecutor<'a> {
    benchmark: i32,
    config: SolverConfig,
    connection: Connection,
    solvers: IDMap,
    testsets: IDMap,
    ingestors: IngestorMap<'a>,
}

impl<'a> LocalExecutor<'a> {
    /// create a new LocalExecutor instance
    pub fn load(
        connection: Connection,
        config: SolverConfig,
        solvers: IDMap,
        testsets: IDMap,
        benchmark: i32,
        ingestors: IngestorMap<'a>,
    ) -> Result<Self, ExecutorError> {
        Ok(Self {
            connection,
            config,
            ingestors,
            benchmark,
            solvers,
            testsets,
        })
    }

    /// execute jobs concurrently with a thread pool
    #[instrument(skip(self), level = "info")]
    pub fn execute(&mut self) -> Result<(), ExecutorError> {
        // pre compile all globs into matchers
        // NOTE: These are kept seperate from `config` to move them in the final map
        let globs = match self.config.compile_globs() {
            Ok(globs) => globs,
            Err(compile_errors) => {
                for (name, err) in compile_errors {
                    error!("Failed to compile glob for {name}: {err}")
                }

                exit(1)
            }
        };

        // setup custom global thread pool
        let mut builder = ThreadPoolBuilder::new();

        let thread_number = if let Some(Some(threads)) = self
            .config
            .executor
            .parameter
            .as_ref()
            .map(|parameters| parameters.get("threads"))
        {
            if let Some(Ok(thread_number)) = threads.as_u64().map(usize::try_from) {
                if thread_number == 0 {
                    error!("0 threads for thread pool are not possible, falling back to number of CPUs");

                    get_core_num()
                } else {
                    thread_number
                }
            } else if let Some(threads) = threads.as_str() {
                let core_num = get_core_num();
                if threads == "pinned" {
                    info!("Pinning threads to logical CPUs");

                    // TODO: Add config option for fine grained pinning control, this is a late
                    // stage feature

                    // cores are spread over all threads, this is done by pinning threads to CPU from high ->
                    // low with affinity
                    let free_cores = AtomicUsize::new(core_num - 1);
                    builder = builder.start_handler(move |thread_handle| {
                        let selected_core = free_cores.fetch_sub(1, ATOMIC_ORDERING);

                        debug!("Pinning thread-pool thread {thread_handle} to logical CPU {selected_core}");
                        set_thread_affinity([selected_core]).expect("Failed to pin thread to CPU");
                    });
                }

                core_num
            } else {
                error!("{threads:?} is not a valid value for executor.params with local executor");

                get_core_num()
            }
        } else {
            get_core_num()
        };

        builder.num_threads(thread_number).build_global().unwrap();
        debug!("Building thread pool with {thread_number} threads");

        // general counters to provide a progress bar
        let total = AtomicU64::new(0);
        let processed = AtomicU64::new(0);

        // find all files
        self.config
            .tests
            .iter()
            .zip(globs.into_iter())
            // ensure set.solvers is always defined
            // Prepare for thread safety
            .map(|((name, set), glob)| (CowStr::from(name.as_str()), Arc::from(set), glob))
            .flat_map(|(name, set, glob)| {
                let cloned_paths = set.paths.clone();
                let (first, others) = cloned_paths.split_first().unwrap();
                let mut builder = WalkBuilder::new(first);

                // use matcher for filtering
                builder.filter_entry(move |path| glob.is_match(path.path()));
                // add other paths
                others.iter().for_each(|path| {
                    builder.add(path);
                });

                let paths = builder
                    .build()
                    .filter_map(|path| match path {
                        // TODO: Add a warning in the docs for this
                        Ok(path) => Some(path),
                        Err(e) => {
                            warn!("Failed to search for tests for test: {e}");
                            None
                        }
                    })
                    .map(|path| Cow::from(path.into_path()))
                    .collect_vec();

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
                debug!(
                    "Processing {:?} with {solver_name:?} for {name} with timeout {}",
                    file, set.timeout
                );

                // TODO: Another map may be used here to allow for fast access
                // For testing this is sufficient though
                let solver = self.config.solvers.get(solver_name.as_ref()).unwrap();
                let timeout = Duration::from_millis(set.timeout as u64);
                let start = Instant::now();

                // this thread is created after the initial thread and inherits it's CPU affinity
                match Command::new(&solver.exec)
                    .args(solver.params.iter())
                    .args(set.params.as_ref().unwrap_or(&Vec::new()))
                    .arg(file.as_os_str())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                {
                    Ok(mut child) => match child.wait_timeout(timeout).unwrap() {
                        Some(status) => {
                            let mut output = RunOutput::new();
                            output.status = status.code().unwrap_or(i32::MIN);

                            // TODO: Add a lot of error fallback around this, in particular the
                            // unwrap and expect parts that involve io
                            output.runtime = start.elapsed().as_millis();
                            let mut stdout = child.stdout.take().unwrap();
                            stdout
                                .read_to_string(&mut output.stdout)
                                .expect("Failed to read stdout");

                            let mut stderr = child.stderr.take().unwrap();
                            stderr
                                .read_to_string(&mut output.stderr)
                                .expect("Failed to read stderr");

                            debug!("Finished in {} ms | status: {}", output.runtime, status);
                            debug!(test = name.to_string(), solver = solver_name.to_string());

                            let context = RunContext {
                                solver: (
                                    *self.solvers.get(&solver_name.to_string()).unwrap(),
                                    solver_name.clone(),
                                ),
                                testset: (
                                    *self.testsets.get(&name.to_string()).unwrap(),
                                    name.clone(),
                                ),
                                benchmark: self.benchmark,
                                path: file.clone(),
                            };
                            match self
                                .ingestors
                                .get(&solver.ingest)
                                .unwrap()
                                .ingest(output)
                            {
                                Ok(metrics) => {
                                    let mut lock = match self.connection.lock() {
                                        Ok(guard) => guard,
                                        Err(e) => {
                                            error!("Failed to acquire connection guard: {e}");
                                            return;
                                        }
                                    };
                                    debug!("Inserting {metrics:?}...");
                                    let mut tx = match lock.transaction() {
                                        Ok(tx) => tx,
                                        Err(e) => {
                                            error!("Failed to acquire transaction: {e}");

                                            return;
                                        }
                                    };

                                    match metrics.insert(&mut tx, context) {
                                        Ok(id) => {
                                            info!("Saving run {}...", id);
                                            if let Err(e) = tx.commit() {
                                                error!("Failed to commit run {id}")
                                            }
                                        }
                                        Err(e) => error!("Failed to insert metric: {e}"),
                                    };
                                }
                                Err(e) => {
                                    let lossy_filename = file.to_string_lossy();

                                    error!(
                                        solver = solver_name.as_str(),
                                        set = name.as_ref(),
                                        file = lossy_filename.as_ref(),
                                        "Failed to ingest record: {e}"
                                    );
                                }
                            };
                        }
                        None => {
                            // child hasn't exited yet
                            child.kill().unwrap();
                        }
                    },
                    Err(e) => {
                        warn!("Failed with {e}");
                    }
                };

                info!(
                    "Done with [{name}/{solver_name}/{}] {}/{}",
                    file.file_name()
                        .unwrap_or(OsStr::new("?"))
                        .to_string_lossy(),
                    processed.fetch_add(1, ATOMIC_ORDERING) + 1,
                    total.load(ATOMIC_ORDERING)
                );
            });

        info!("Done with processing {} items", total.load(ATOMIC_ORDERING));

        Ok(())
    }
}
