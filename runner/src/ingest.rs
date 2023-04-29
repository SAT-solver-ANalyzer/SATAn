pub mod exec;

use crate::{
    config::{ConfigErrors, IngestorConfig},
    database::{util::IDMap, TestMetrics},
};
use cowstr::CowStr;
use std::{borrow::Cow, collections::BTreeMap, path::Path};
use thiserror::Error;

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
    pub benchmark: i32,
    pub path: Cow<'a, Path>,
    pub solver: (i32, CowStr),
    pub testset: (i32, CowStr),
}

impl<'a> RunContext<'a> {
    pub fn new(
        path: Cow<'a, Path>,
        solver_name: &CowStr,
        name: &CowStr,
        solvers: &IDMap,
        testsets: &IDMap,
        benchmark: i32,
    ) -> Self {
        Self {
            solver: (
                *solvers.get(solver_name.as_str()).unwrap(),
                solver_name.clone(),
            ),
            testset: (*testsets.get(name.as_str()).unwrap(), name.clone()),
            benchmark,
            path: path.clone(),
        }
    }
}

pub type IngestorMap<'a> = BTreeMap<CowStr, Ingestors<'a>>;

#[derive(Clone, Debug)]
pub enum Ingestors<'a> {
    Raw(exec::RawIngestor<'a>),
}

impl Ingestors<'_> {
    pub fn load(config: &IngestorConfig) -> Result<Self, ConfigErrors> {
        match config.name.to_lowercase().as_str() {
            "raw" => exec::RawIngestor::load(config).map(Ingestors::Raw),
            _ => Err(ConfigErrors::FailedLoadIngestor),
        }
    }

    #[tracing::instrument(level = "debug")]
    pub fn ingest(&self, output: RunOutput) -> Result<TestMetrics, IngestorError> {
        match self {
            Self::Raw(ingestor) => ingestor.ingest(output),
        }
    }
}
