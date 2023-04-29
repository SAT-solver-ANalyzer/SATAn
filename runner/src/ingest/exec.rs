use super::{IngestorError, RunOutput};
use crate::{
    config::{ConfigErrors, IngestorConfig},
    database::TestMetrics,
};
use std::{
    borrow::Cow,
    ffi::OsStr,
    io::{Read, Write},
    process::{Command, Stdio},
    time::Duration,
};
use tracing::{debug, error, info, warn};
use tracing_unwrap::OptionExt;
use tracing_unwrap::ResultExt;
use wait_timeout::ChildExt;

#[derive(Debug, Clone)]
pub struct RawIngestor<'a> {
    pub ingestor: Cow<'a, OsStr>,
    pub params: Cow<'a, OsStr>,
    pub timeout: Duration,
}

impl<'a> RawIngestor<'a> {
    pub fn load(config: &IngestorConfig) -> Result<Self, ConfigErrors> {
        if let Some(Some(exec)) = config.parameter.get("exec").map(|exec| exec.as_str()) {
            let timeout = Duration::from_millis(match config.parameter.get("timeout") {
                Some(timeout_value) => match timeout_value.as_u64() {
                    Some(value) => value,
                    None => {
                        warn!("Ingestor timeout must be a natural number");
                        return Err(ConfigErrors::FailedLoadIngestor);
                    }
                },
                None => 2000,
            });

            let params = OsStr::new(match config.parameter.get("params") {
                Some(value) => match value.as_str() {
                    Some(value) => value,
                    None => {
                        warn!("Ingestor params must be a string");
                        return Err(ConfigErrors::FailedLoadIngestor);
                    }
                },
                None => "",
            })
            .to_owned();

            Ok(Self {
                ingestor: Cow::from(OsStr::new(exec).to_owned()),
                timeout,
                // TODO: make below prettier
                params: Cow::from(params),
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
                let mut stdin = handle.stdin.take().unwrap_or_log();

                stdin
                    .write_all(output.stdout.as_bytes())
                    .expect("Failed to write to stdin pipe of ingestor");
                // Dropping stdin here will close the underlying file descriptor
                // this makes writing ingestors easier as they have a clear end of input for stind
                drop(stdin);

                debug!("Ingestor waiting on {}", handle.id());
                let status = match handle.wait_timeout(self.timeout).unwrap_or_log() {
                    Some(status) => {
                        debug!("Ingestor exit status: {status:?}");

                        status.success()
                    }
                    None => {
                        debug!("Ingestor ran into timeout, attempting to continue");

                        return Err(super::IngestorError::ChildTimeout);
                    }
                };

                if !status {
                    let mut stderr_buffer = String::new();

                    if let Some(mut stderr) = handle.stderr.take() {
                        stderr.read_to_string(&mut stderr_buffer).unwrap();
                    }

                    debug!(
                        stderr = stderr_buffer,
                        message = "Ingestor failed to ingest input, attempting to continue"
                    );
                }

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
