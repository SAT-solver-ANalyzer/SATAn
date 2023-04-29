mod local;

use crate::{
    config::{ConfigErrors, SolverConfig},
    database::{util::IDMap, Connection},
    ingest::{IngestorError, IngestorMap},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Ingest step failed")]
    IngestError(#[from] IngestorError),
}

#[derive(Clone, Debug)]
pub enum Executors<'a> {
    Local(local::LocalExecutor<'a>),
}

impl<'a> Executors<'a> {
    pub fn load(
        connection: Connection,
        config: SolverConfig,
        ingestors: IngestorMap<'a>,
        solvers: IDMap,
        testsets: IDMap,
        benchmark: i32,
    ) -> Result<Self, ConfigErrors> {
        match config.executor.name.as_str() {
            "local" => Ok(Self::Local(local::LocalExecutor::load(
                connection, config, solvers, testsets, benchmark, ingestors,
            )?)),
            _ => Err(ConfigErrors::UnsupportedExecutor(
                config.executor.name.to_string(),
            )),
        }
    }

    pub fn execute(&mut self) -> Result<(), ExecutorError> {
        match self {
            Self::Local(executor) => executor.execute(),
        }
    }
}
