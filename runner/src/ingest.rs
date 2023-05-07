pub mod exec;

use crate::{
    config::{ConfigErrors, IngestorConfig},
    database::TestMetrics,
};
use cowstr::CowStr;
use std::{borrow::Cow, collections::BTreeMap, path::Path};
use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum IngestorError {
    // TODO: Integrate for specific error, once lifetimes are clearly defined
    #[error("Failed to spawn ingestor")]
    SpawnIngestor(std::io::Error),
    #[error("Failed to deserialize ingestor output")]
    DeserializeIngestor(#[from] serde_yaml::Error),
    #[error("Failed to wait for a child proccess")]
    ChildError(#[from] std::io::Error),
    #[error("Ingestor timeout")]
    ChildTimeout,
}

#[derive(Debug, Clone)]
/// container for information extracted from running a solver
/// supposed to be interpreted by ingestors
pub struct RunOutput {
    pub runtime: u128,
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

impl RunOutput {
    pub fn new() -> Self {
        Self {
            runtime: 0,
            stdout: String::new(),
            stderr: String::new(),
            status: 0,
        }
    }
}

#[derive(Debug, Clone)]
/// container for information related to running a solver
/// e.g., solver id, benchmark id, test set id, ...
pub struct RunContext<'a> {
    pub path: Cow<'a, Path>,
    pub benchmark: i32,
    pub solver: (i32, CowStr),
    pub testset: (i32, CowStr),
}

pub type IngestorMap<'a> = BTreeMap<CowStr, Ingestors<'a>>;

#[derive(Clone, Debug)]
pub enum Ingestors<'a> {
    Exec(exec::ExecIngestor<'a>),
    Null,
}

impl Ingestors<'_> {
    // TODO: Abstract below into a trait
    pub fn load(config: &IngestorConfig) -> Result<Self, ConfigErrors> {
        match config {
            IngestorConfig::Null => Ok(Self::Null),
            IngestorConfig::Exec { .. } => {
                exec::ExecIngestor::load(config).map(|exec| Ingestors::Exec(exec))
            }
        }
    }

    #[tracing::instrument(level = "debug")]
    pub fn ingest(&self, output: RunOutput) -> Result<TestMetrics, IngestorError> {
        match self {
            Self::Exec(ingestor) => ingestor.ingest(output),
            Self::Null => match serde_yaml::from_str(&output.stdout) {
                Ok(metrics) => Ok(metrics),
                Err(error) => {
                    error!(error = ?error, "Failed to deserialize metrics for null ingestor");

                    Err(IngestorError::DeserializeIngestor(error))
                }
            },
        }
    }
}
