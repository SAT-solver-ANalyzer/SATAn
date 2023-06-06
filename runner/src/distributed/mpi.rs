use crate::collector::Collector;

#[derive(Debug)]
pub struct MPICoordinator {
    pub is_master: bool,
}

#[derive(Debug, Clone)]
pub struct MPICollector {
    inner: Box<Collector>,
}

impl MPICollector {
    pub fn new(collector: Collector) -> Self {
        Self {
            inner: Box::new(collector),
        }
    }
}

#[derive(Debug)]
pub struct MPIStorage {}
