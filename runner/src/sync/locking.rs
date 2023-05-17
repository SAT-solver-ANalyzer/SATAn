use crate::executors;

#[derive(Debug)]
pub struct LockingExecutor<'a> {
    pub local: executors::local::LocalExecutor<'a>,
}

/* 1. Collect tasks
 * 2. Try to acquire tasks and dispatch to local thread pool
 * 2. potential issues:
 *   - contention when trying to acquire work with a high throughput test set
 *   - Slow metadata server: Assuming a excl lock is used the issue above will be a PITA for lustre
 *   of bgfs metadata server
 * */

impl<'a> LockingExecutor<'a> {}

/*
 * For distribued work with SQLite
 * -> Each runner creates their own database and collects it's outputs into it
 * -> Either manually or at the end all databases are merged by the benchmark id
 *
 * Challenges:
 * - to make this sound a lot of stat calls are required this will strain metadata servers for
 * e.g., lustre a lot and may have a high mimpact on the general responsiblity of the system
 *
 * Recommendations:
 * - use this in an environment where MPI is not available or for tests with low throughput
 *
 * The work has to be processed in two steps.
 * 1. walk over files from collector
 * 2. try exclusive lock -> if possible k
 */
