use super::{Connection, IngestorError};
use crate::config::{ConfigErrors, IngestorConfig};
use std::borrow::Cow;
use tracing::error;

#[derive(Debug, Clone)]
pub struct RawIngestor<'a> {
    pub connection: Connection,
    pub ingestor: Cow<'a, String>,
}

impl<'a> RawIngestor<'a> {
    pub fn load(
        config: &IngestorConfig,
        connection: Connection,
    ) -> Result<Self, ConfigErrors> {
        if let Some(Some(exec)) = config.parameter.get("exec").map(|exec| exec.as_str()) {
            Ok(Self {
                connection,
                ingestor: Cow::Owned(exec.to_owned()),
            })
        } else {
            error!("The raw executor requires ingestor.exec to be a str pointing to the path of the ingestor script");

            Err(ConfigErrors::FailedLoadIngestor)
        }
    }

    pub fn ingest(
        &self,
        status: bool,
        stdout: Option<String>,
        stderr: Option<String>,
    ) -> Result<super::Test, super::IngestorError> {
        let lock = match self.connection.lock() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire connection guard: {e}");
                return Err(IngestorError::ConnectionError);
            }
        };

        // Explicitly drop here to avoid posioning the lock when todo panics
        drop(lock);

        todo!()
    }
}
