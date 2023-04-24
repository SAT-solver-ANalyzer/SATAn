mod local;

use crate::{
    config::{ConfigErrors, SolverConfig},
    ingest::IngestorMap,
};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ExecutorError {}

#[derive(Clone, Debug)]
pub enum Executors<'a> {
    Local(local::LocalExecutor<'a>),
}

impl<'a> Executors<'a> {
    pub fn load(
        config: SolverConfig,
        ingestors: IngestorMap<'a>,
    ) -> Result<Self, ConfigErrors> {
        match config.executor.name.as_str() {
            "local" => Ok(Self::Local(local::LocalExecutor::load(config, ingestors)?)),
            _ => Err(ConfigErrors::UnsupportedExecutor(config.executor.name)),
        }
    }

    pub fn execute(&mut self) -> Result<(), ExecutorError> {
        match self {
            Self::Local(executor) => executor.execute(),
        }
    }
}
