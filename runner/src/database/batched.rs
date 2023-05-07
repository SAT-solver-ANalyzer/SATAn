use super::{duckdb::InnerConnection, ConnectionError, TestMetrics, ID};
use crate::config::{DatabaseConfig, SolverConfig};
use cowstr::CowStr;
use parking_lot::FairMutex;
use std::{path::PathBuf, sync::Arc};
use tracing::error;
use tracing_unwrap::ResultExt;

#[derive(Debug)]
pub struct MetricsBundle {
    pub metrics: TestMetrics,
    pub solver: CowStr,
    pub test_set: CowStr,
    pub target: PathBuf,
}

#[derive(Debug)]
pub struct BatchedConnection {
    connection: Arc<FairMutex<InnerConnection>>,
    buffer: Arc<FairMutex<Vec<MetricsBundle>>>,
    size: u32,
}

impl BatchedConnection {
    pub fn load(config: &DatabaseConfig) -> Result<Self, ConnectionError> {
        match config {
            DatabaseConfig::Batched { path: _, size } => {
                if *size <= 1 {
                    error!("The batch size needs to be >= 1");

                    Err(ConnectionError::ConfigError)
                } else {
                    Ok(Self {
                        connection: Arc::new(FairMutex::new(InnerConnection::load(config)?)),
                        buffer: Arc::new(FairMutex::new(Vec::new())),
                        size: *size,
                    })
                }
            }
            _ => unreachable!(),
        }
    }

    pub fn init(
        &mut self,
        config: &SolverConfig,
        benchmark: Option<ID>,
        comment: Option<String>,
    ) -> Result<(), ConnectionError> {
        self.connection.lock_arc().init(config, benchmark, comment)
    }

    pub fn close(self) -> Result<(), ConnectionError> {
        let buffer = self.buffer.lock_arc();

        let mut connection = Arc::try_unwrap(self.connection)
            .unwrap_or_log()
            .into_inner();

        if buffer.len() > 0 {
            connection.store_iter(buffer.iter())?;
        }

        connection.close()
    }

    pub fn store(
        &self,
        metrics: TestMetrics,
        solver: CowStr,
        test_set: CowStr,
        target: &PathBuf,
    ) -> Result<i32, ConnectionError> {
        let mut buffer = self.buffer.lock_arc();

        if buffer.len() as u32 == self.size {
            self.connection.lock_arc().store_iter(buffer.iter())?;

            buffer.clear();
        } else {
            buffer.push(MetricsBundle {
                metrics,
                solver,
                test_set,
                target: target.clone(),
            });
        }

        Ok(i32::MIN)
    }
}
