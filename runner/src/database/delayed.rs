use super::{ConnectionAdapters, ConnectionError, MetricsBundle, TestMetrics, ID};
use crate::config::SolverConfig;
use cowstr::CowStr;
use parking_lot::FairMutex;
use std::{path::PathBuf, sync::Arc};
use tracing_unwrap::ResultExt;

#[derive(Debug)]
pub struct DelayedConnection {
    adapter: Box<ConnectionAdapters>,
    buffer: Arc<FairMutex<Vec<MetricsBundle>>>,
}

impl DelayedConnection {
    pub fn init(
        &mut self,
        config: &SolverConfig,
        benchmark: Option<ID>,
        comment: Option<String>,
    ) -> Result<(), ConnectionError> {
        self.adapter.init(config, benchmark, comment)
    }

    pub fn load(connection: ConnectionAdapters) -> Self {
        Self {
            buffer: Arc::new(FairMutex::new(Vec::new())),
            adapter: Box::new(connection),
        }
    }

    pub fn close(self) -> Result<(), ConnectionError> {
        let buffer = Arc::try_unwrap(self.buffer).unwrap_or_log().into_inner();

        if !buffer.is_empty() {
            self.adapter.store_iter(buffer.into_iter())?;
        }

        self.adapter.close()
    }

    pub fn store(
        &self,
        metrics: TestMetrics,
        solver: CowStr,
        test_set: CowStr,
        target: &PathBuf,
    ) -> Result<i32, ConnectionError> {
        self.buffer.lock_arc().push(MetricsBundle {
            metrics,
            solver,
            test_set,
            target: target.to_path_buf(),
        });

        Ok(i32::MIN)
    }
}
