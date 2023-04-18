use super::{Executor, ExecutorError};
use crate::config::SolverConfig;
use affinity::{get_core_num, set_thread_affinity};
use ignore::WalkBuilder;
use itertools::{iproduct, Itertools};
use rayon::{prelude::*, ThreadPoolBuilder};
use std::{
    borrow::Cow,
    io::Read,
    process::{exit, Command, Stdio},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tracing::{debug, error, info, instrument, trace, warn};
use wait_timeout::ChildExt;

const ATOMIC_ORDERING: Ordering = Ordering::SeqCst;

/// Executor that works on a local rayon-backed thread pool
pub struct LocalExecutor {
    config: SolverConfig,
}

impl Executor for LocalExecutor {
    /// create a new LocalExecutor instance
    fn load(config: SolverConfig) -> Result<Self, ExecutorError> {
        Ok(Self { config })
    }

    /// execute jobs concurrently with a thread pool
    #[instrument(skip(self), level = "info")]
    fn execute(&mut self) -> Result<(), ExecutorError> {
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
            .map(|((name, set), glob)| (Cow::from(name), Arc::from(set), glob))
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
                    .map(|path| path.into_path().as_os_str().to_owned())
                    .collect_vec();

                // increase total counter for progress bar
                total.fetch_add((paths.len() * set.solvers.len()) as u64, ATOMIC_ORDERING);

                // create actual tasks for all sets x solvers, including test metadata for
                // ingesting
                iproduct!(paths, set.solvers.clone())
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
                let solver = self.config.solvers.get(&solver_name).unwrap();
                let timeout = Duration::from_secs(set.timeout as u64);
                let start = Instant::now();

                // this thread is created after the initial thread and inherits it's CPU affinity
                match Command::new(&solver.exec)
                    .args(solver.params.iter())
                    .args(set.params.as_ref().unwrap_or(&[].to_vec()))
                    .arg(file)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                {
                    Ok(mut child) => match child.wait_timeout(timeout).unwrap() {
                        Some(status) => {
                            let elapsed = start.elapsed();
                            let mut stdout = child.stdout.take().unwrap();
                            let mut output = String::new();
                            stdout.read_to_string(&mut output).expect("Failed to read");

                            debug!(
                                "Finished in {} ns | status: {}",
                                elapsed.as_nanos(),
                                status.success()
                            );
                            trace!("Output: {output}");
                            // TODO: Here is the part where ingest needs to be attached
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
                    "Done with {}/{}",
                    processed.fetch_add(1, ATOMIC_ORDERING) + 1,
                    total.load(ATOMIC_ORDERING)
                );
            });

        info!("Done with processing {} items", total.load(ATOMIC_ORDERING));

        Ok(())
    }
}
