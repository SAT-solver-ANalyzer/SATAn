/// # The database schema
/// The schema is written to account for a dynamic set of metrics.
/// These are supplied by the ingestor during the preflight-phase
///
///                                         ┌─────────┐
///                              ┌─────────►│Test Sets│
///                              │          └─────────┘
///                              │
///                              │          ┌───────┐
///                              ├─────────►│Solvers│
///                              │          └───────┘
///                              │
/// ┌───────────────────┐    ┌───┤          ┌─────────┐
/// │{ingestor}_metrics │◄───┤Run├─────────►│Ingestors│
/// └───────────────────┘    └───┤          └─────────┘
///                              │
///                              │          ┌──────────┐
///                              └─────────►│Benchmarks│
///                                         └──────────┘
use cowstr::CowStr;
use std::collections::BTreeMap;

pub type IDMap = BTreeMap<CowStr, i32>;


