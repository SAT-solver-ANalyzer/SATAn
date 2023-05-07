/*
 * Plan for the slurm executor:
 * 1. Retrieve jobs -> turn into iterable
 * 2. Create batches, i.e., a window over the iterable
 * 3. Create empty runs and prepare params for slurm jobs
 * 4. Create Slurm jobs, pack into job arrays is possible set a max limit for the number of jobs
 *   4.1 Thsi should work either with sbatch or the Slurm REST API
 */

use crate::database::StorageAdapters;

#[derive(Debug)]
pub struct SlurmExecutor {
    connection: StorageAdapters,
}

impl SlurmExecutor {}
