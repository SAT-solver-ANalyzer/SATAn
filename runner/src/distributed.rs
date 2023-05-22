pub mod mpi;
pub mod util;

/*
 * Plan for the slurm executor:
 * 1. Retrieve jobs -> turn into iterable
 * 2. Create batches, i.e., a window over the iterable
 * 3. Create empty runs and prepare params for slurm jobs
 * 4. Create Slurm jobs, pack into job arrays is possible set a max limit for the number of jobs
 *   4.1 Thsi should work either with sbatch or the Slurm REST API
 *
 *
 *
 *
 * Distributed work scheduling: two operating modes
 * -> coordinator-less handling of work with file locking
 *   -> (ab)use SQLite as locking handler for work
 * -> work with coordination over MPI where a single node is the coordinator
 * -> coordination is done out of band and usually with a moderatley sized queue
 *
 *
 * 1. Use collector -> Get some kind of file structure
 * 2. Start distributed, coordinator-less runner cluster
 * 3. Start to work by fetching data from the structure
 * 4. Communicate the changes and allow for some kind of cooperative iteration execution
 *
 * Battle plan:
 * 1. Get the main strucutre for work stealing in order
 * 2. Hit the ground running -> identify the problems and go on
 */

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub enum SynchronizationTypes {
    Coordinated,
    FileSystem { path: PathBuf },
}
