use super::{ConnectionAdapter, ConnectionError, MetricsBundle, TestMetrics, ID};
use crate::config::{BatchConfig, SolverConfig};
use cowstr::CowStr;
use parking_lot::FairMutex;
use std::{path::PathBuf, sync::Arc};
use tracing_unwrap::ResultExt;

#[derive(Debug)]
pub struct BatchedConnection {
    connection: Box<ConnectionAdapter>,
    buffer: Arc<FairMutex<Vec<MetricsBundle>>>,
    size: u32,
}

impl BatchedConnection {
    pub fn load(config: &BatchConfig, connection: ConnectionAdapter) -> Self {
        Self {
            buffer: Arc::new(FairMutex::new(Vec::new())),
            size: config.size,
            connection: Box::new(connection),
        }
    }

    pub fn init(
        &mut self,
        config: &SolverConfig,
        benchmark: Option<ID>,
        comment: Option<String>,
    ) -> Result<(), ConnectionError> {
        self.connection.init(config, benchmark, comment)
    }

    pub fn close(self) -> Result<(), ConnectionError> {
        let buffer = Arc::try_unwrap(self.buffer).unwrap_or_log().into_inner();

        if !buffer.is_empty() {
            self.connection.store_iter(buffer.into_iter())?;
        }

        self.connection.close()
    }

    pub fn store(
        &self,
        metrics: TestMetrics,
        solver: CowStr,
        test_set: CowStr,
        target: &PathBuf,
    ) -> Result<i32, ConnectionError> {
        let mut buffer = self.buffer.lock_arc();

        buffer.push(MetricsBundle {
            metrics,
            solver,
            test_set,
            target: target.clone(),
        });

        if buffer.len() as u32 == self.size {
            self.connection.store_iter(buffer.drain(0..))?;

            buffer.clear();
        }

        Ok(i32::MIN)
    }
}
