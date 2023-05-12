pub mod coordinator;
pub mod locking;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub enum SynchronizationTypes {
    Coordinated,
    FileSystem { path: PathBuf },
}
