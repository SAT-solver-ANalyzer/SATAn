pub mod exec;

use crate::{
    config::{ConfigErrors, IngestorConfig},
    database::{Connection, Test},
};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum IngestorError {
    // TODO: Integrate for specific error, once lifetimes are clearly defined
    #[error("Failed to load connection")]
    ConnectionError,
}

pub type IngestorMap<'a> = BTreeMap<String, Ingestors<'a>>;

#[derive(Clone, Debug)]
pub enum Ingestors<'a> {
    Raw(exec::RawIngestor<'a>),
}

impl Ingestors<'_> {
    pub fn load(config: &IngestorConfig, connection: Connection) -> Result<Self, ConfigErrors> {
        match config.name.to_lowercase().as_str() {
            "raw" => {
                exec::RawIngestor::load(config, connection).map(|ingestor| Ingestors::Raw(ingestor))
            }
            _ => Err(ConfigErrors::FailedLoadIngestor),
        }
    }

    #[tracing::instrument(level = "debug")]
    pub fn ingest(
        &self,
        status: i32,
        stdout: String,
        stderr: String,
    ) -> Result<Test, IngestorError> {
        Err(IngestorError::ConnectionError)
    }
}
