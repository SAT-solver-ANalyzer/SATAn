mod jobs;
mod local;
mod slurm;

use crate::{
    config::{ConfigErrors, ExecutorConfig, SolverConfig},
    database::{ConnectionError, StorageAdapters},
    ingest::{IngestorError, IngestorMap},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Ingest step failed")]
    IngestError(#[from] IngestorError),
    #[error("Metric storage failed")]
    ConnectionError(#[from] ConnectionError),
}

#[derive(Debug)]
pub enum Executors<'a> {
    Local(local::LocalExecutor<'a>),
    Slurm(slurm::SlurmExecutor),
}

impl<'a> Executors<'a> {
    pub fn load(
        connection: StorageAdapters,
        config: SolverConfig,
        ingestors: IngestorMap<'a>,
    ) -> Result<Self, ConfigErrors> {
        match config.executor {
            ExecutorConfig::Local { .. } => Ok(Self::Local(local::LocalExecutor::load(
                connection, config, ingestors,
            )?)),
        }
    }

    pub fn execute(self) -> Result<(), ExecutorError> {
        match self {
            Self::Local(executor) => executor.execute(),
            Self::Slurm { .. } => todo!(),
        }
    }
}
