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
