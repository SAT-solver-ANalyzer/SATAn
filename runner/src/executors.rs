mod local;

use crate::config::{ConfigErrors, SolverConfig};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ExecutorError {}

/// Trait for handling all builtin executors
pub trait Executor: Sized {
    fn load(config: SolverConfig) -> Result<Self, ExecutorError>;
    fn execute(&mut self) -> Result<(), ExecutorError>;
}

#[derive(Clone, Debug)]
pub struct Executors;

impl Executors {
    pub fn load(config: SolverConfig) -> Result<impl Executor, ConfigErrors> {
        match config.executor.name.to_lowercase().as_str() {
            "local" => Ok(local::LocalExecutor::load(config)?),
            _ => Err(ConfigErrors::UnsupportedExecutor(config.executor.name)),
        }
    }
}
