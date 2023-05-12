#[cfg(feature = "distributed")]
mod distributed;
pub mod local;

#[cfg(feature = "distributed")]
use crate::sync::locking::LockingExecutor;
use crate::{
    collector::TestCollector,
    config::{ConfigErrors, ExecutorConfig, SolverConfig},
    database::{ConnectionAdapters, ConnectionError},
    ingest::{IngestorError, IngestorMap},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Ingest step failed")]
    IngestError(#[from] IngestorError),
    #[error("Metric storage failed")]
    ConnectionError(#[from] ConnectionError),
    #[error("Failed to build collector globs")]
    GlobError(#[from] globset::Error),
}

#[derive(Debug)]
pub enum Executors<'a> {
    Local(local::LocalExecutor<'a>),

    #[cfg(feature = "distributed")]
    Distributed(LockingExecutor<'a>),
}

impl<'a> Executors<'a> {
    pub fn load(
        connection: ConnectionAdapters,
        config: SolverConfig,
        ingestors: IngestorMap<'a>,
        collectors: Vec<TestCollector>,
    ) -> Result<Self, ConfigErrors> {
        match config.executor {
            ExecutorConfig::Local { .. } => Ok(Self::Local(local::LocalExecutor::load(
                connection, config, ingestors, collectors,
            )?)),

            #[cfg(feature = "distributed")]
            ExecutorConfig::Distributed { .. } => Ok(Self::Distributed(LockingExecutor {
                local: local::LocalExecutor::load(connection, config, ingestors, collectors)?,
            })),
        }
    }

    pub fn execute(self) -> Result<(), ExecutorError> {
        match self {
            Self::Local(executor) => executor.execute(),
            #[cfg(feature = "distributed")]
            Self::Distributed { .. } => todo!(),
        }
    }
}
