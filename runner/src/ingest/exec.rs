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
use tracing::{debug, error, trace, warn};
use tracing_unwrap::OptionExt;
use tracing_unwrap::ResultExt;
use wait_timeout::ChildExt;

#[derive(Debug, Clone)]
pub struct ExecIngestor<'a> {
    pub ingestor: Cow<'a, OsStr>,
    pub params: Cow<'a, OsStr>,
    pub timeout: Duration,
}

impl<'a> ExecIngestor<'a> {
    pub fn load(config: &IngestorConfig) -> Result<Self, ConfigErrors> {
        match config {
            IngestorConfig::Exec(config) => Ok(Self {
                ingestor: Cow::from(config.executable.as_os_str().to_owned()),
                timeout: Duration::from_millis(config.timeout),
                params: Cow::from(OsStr::new(config.params.as_str().clone()).to_owned()),
            }),
            _ => unreachable!(),
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

                trace!("Output from ingestor: {buffer}");

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
