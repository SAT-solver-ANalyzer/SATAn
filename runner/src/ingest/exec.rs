use super::{IngestorError, RunContext, RunOutput};
use crate::{
    config::{ConfigErrors, IngestorConfig},
    database::TestMetrics,
};
use std::{
    borrow::Cow,
    ffi::OsStr,
    io::{Read, Write},
    process::{Command, Stdio},
};
use tracing::{debug, error};

#[derive(Debug, Clone)]
pub struct RawIngestor<'a> {
    pub ingestor: Cow<'a, OsStr>,
    pub params: Cow<'a, OsStr>,
}

impl<'a> RawIngestor<'a> {
    pub fn load(config: &IngestorConfig) -> Result<Self, ConfigErrors> {
        if let Some(Some(exec)) = config.parameter.get("exec").map(|exec| exec.as_str()) {
            Ok(Self {
                ingestor: Cow::from(OsStr::new(exec).to_owned()),
                // TODO: make below prettier
                params: Cow::from(
                    OsStr::new(
                        config
                            .parameter
                            .get("params")
                            .map(|params| params.as_str())
                            .unwrap_or(Some(""))
                            .unwrap_or(""),
                    )
                    .to_owned(),
                ),
            })
        } else {
            error!("The raw executor requires ingestor.exec to be a str pointing to the path of the ingestor script");

            Err(ConfigErrors::FailedLoadIngestor)
        }
    }

    #[tracing::instrument(level = "debug")]
    pub fn ingest(&self, output: RunOutput) -> Result<TestMetrics, super::IngestorError> {
        // TODO: add configurable timeout for ingestors, otherwiese they might lock a connection
        match Command::new(&self.ingestor)
            .arg(&self.params)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn()
        {
            Ok(mut handle) => {
                let mut stdin = handle.stdin.take().unwrap();

                stdin
                    .write_all(output.stdout.as_bytes())
                    .expect("Failed to write to stdin pipe of ingestor");
                drop(stdin);

                debug!("Ingestor waiting on {}", handle.id());
                let status = handle.wait()?;
                debug!("{status:?}");

                // retrieve output from ingestor
                let mut output = handle
                    .stdout
                    .take()
                    .expect("Failed to acquire stdout of ingestor handle");
                let mut buffer = String::new();
                output
                    .read_to_string(&mut buffer)
                    .expect("Failed to read output");

                debug!("Output from ingestor: {buffer}");

                match serde_yaml::from_str::<TestMetrics>(&buffer) {
                    Ok(metrics) => Ok(metrics),
                    Err(e) => {
                        error!("Ingestor failed to read metrics: {e}");

                        Err(e.into())
                    }
                }
            }
            Err(e) => Err(IngestorError::SpawnIngestor(e)),
        }
    }
}
