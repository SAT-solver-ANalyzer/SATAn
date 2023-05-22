pub mod batched;
#[cfg(feature = "clickhouse")]
pub mod clickhouse;
pub mod delayed;
#[cfg(feature = "duckdb")]
pub mod duckdb;
#[cfg(feature = "rusqlite")]
pub mod sqlite;
pub mod util;

use crate::config::{ConnectionConfig, DatabaseConfig, SolverConfig};
use cowstr::CowStr;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::{fmt::Debug, path::PathBuf};
use thiserror::Error;

// MID TERM: add clickhouse
// LONG TERM: add exporter (CSV) and migrator (duckdb <-> clickhouse)

// Alias for all database IDs for benchmarks, solvers and testsets
// This might be upped to an i64 if the demand ever arises
pub type ID = i32;

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug, Clone)]
#[repr(i8)]
pub enum Satisfiability {
    Unsatisfiable = -1,
    Unknown = 0,
    Satisfiable = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMetrics {
    // TODO: figure out more relevant metrics, either compare between the outputs from the SAT
    // solvers or figure out what dimensions are relevant (RAM usage, L1/L2/L3 Cache usage, ...)
    pub runtime: u64,
    pub satisfiable: Satisfiability,
    pub parse_time: u64,
    pub memory_usage: u32,
    pub restarts: u32,
    pub conflicts: u32,
    pub propagations: u32,
    pub conflict_literals: u32,
    pub number_of_variables: u32,
    pub number_of_clauses: u32,
}

#[derive(Debug)]
pub struct MetricsBundle {
    pub metrics: TestMetrics,
    pub solver: CowStr,
    pub test_set: CowStr,
    pub target: PathBuf,
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[cfg_attr(feature = "duckdb", error("DuckDB adapter error"))]
    #[cfg(feature = "duckdb")]
    DuckDB(duckdb::DuckDBError),
    #[cfg_attr(feature = "rusqlite", error("SQLite adapter error"))]
    #[cfg(feature = "rusqlite")]
    SQLite(rusqlite::Error),
    #[error("Invalid adapter configuration")]
    ConfigError,
}

#[derive(Debug)]
pub enum ConnectionAdapter {
    #[cfg(feature = "duckdb")]
    DuckDB(duckdb::SharedConnection),
    #[cfg(feature = "rusqlite")]
    SQLite(sqlite::SharedConnection),
    Batched(batched::BatchedConnection),
    Delayed(delayed::DelayedConnection),
    #[cfg(feature = "clickhouse")]
    ClickHouse(clickhouse::CHConnection),
}

/// Trait abstracting the interface to a database
impl ConnectionAdapter {
    /// Insert a single run output entry
    pub fn store(
        &self,
        metrics: TestMetrics,
        solver: CowStr,
        test_set: CowStr,
        target: &PathBuf,
    ) -> Result<ID, ConnectionError> {
        match self {
            #[cfg(feature = "duckdb")]
            Self::DuckDB(shared_connection) => {
                shared_connection.store(metrics, solver, test_set, target)
            }
            Self::Batched(shared_connection) => {
                shared_connection.store(metrics, solver, test_set, target)
            }
            #[cfg(feature = "rusqlite")]
            Self::SQLite(shared_connection) => {
                shared_connection.store(metrics, solver, test_set, target)
            }
            Self::Delayed(delayed) => delayed.store(metrics, solver, test_set, target),
            #[cfg(feature = "clickhouse")]
            Self::ClickHouse { .. } => todo!(),
        }
    }

    /// Close the connection and ensure consistency (i.e., finish batched inserts)
    pub fn close(self) -> Result<(), ConnectionError> {
        match self {
            #[cfg(feature = "duckdb")]
            Self::DuckDB(shared_connection) => shared_connection.close(),
            #[cfg(feature = "rusqlite")]
            Self::SQLite(shared_connection) => shared_connection.close(),
            Self::Batched(shared_connection) => shared_connection.close(),
            Self::Delayed(shared_connection) => shared_connection.close(),
            #[cfg(feature = "clickhouse")]
            Self::ClickHouse { .. } => todo!(),
        }
    }

    /// Estabilish a connection without wrapped types
    pub fn load_connection(config: &DatabaseConfig) -> Result<Self, ConnectionError> {
        match config.connection {
            #[cfg(feature = "duckdb")]
            ConnectionConfig::DuckDB { .. } => {
                duckdb::SharedConnection::load(&config.connection).map(Self::DuckDB)
            }
            #[cfg(feature = "rusqlite")]
            ConnectionConfig::SQLite { .. } => {
                sqlite::SharedConnection::load(&config.connection).map(Self::SQLite)
            }
            #[cfg(feature = "clickhouse")]
            DatabaseConfig::ClickHouse { .. } => todo!(),
        }
    }

    /// Estabilish a connection to the specified database with optional wrapped types
    pub fn load(config: &DatabaseConfig) -> Result<Self, ConnectionError> {
        if config.delayed {
            Ok(Self::Delayed(delayed::DelayedConnection::load(
                Self::load_connection(config)?,
            )))
        } else if let Some(batched_config) = &config.batched {
            Ok(Self::Batched(batched::BatchedConnection::load(
                batched_config,
                Self::load_connection(config)?,
            )))
        } else {
            Self::load_connection(config)
        }
    }

    /// Do any initialization, if applicable, like loading SQL schemas and creating solver/tests
    /// entries
    pub fn init(
        &mut self,
        config: &SolverConfig,
        benchmark: Option<ID>,
        comment: Option<String>,
    ) -> Result<(), ConnectionError> {
        match self {
            #[cfg(feature = "duckdb")]
            Self::DuckDB(shared_connection) => shared_connection.init(config, benchmark, comment),
            Self::Batched(shared_connection) => shared_connection.init(config, benchmark, comment),
            #[cfg(feature = "rusqlite")]
            Self::SQLite(shared_connection) => shared_connection.init(config, benchmark, comment),
            Self::Delayed(shared_connection) => shared_connection.init(config, benchmark, comment),
            #[cfg(feature = "clickhouse")]
            Self::ClickHouse { .. } => todo!(),
        }
    }

    pub fn store_iter<'a, I: Iterator<Item = MetricsBundle>>(
        &self,
        metrics: I,
    ) -> Result<(), ConnectionError> {
        match self {
            #[cfg(feature = "duckdb")]
            Self::DuckDB(shared_connection) => shared_connection.store_iter(metrics),
            #[cfg(feature = "rusqlite")]
            Self::SQLite(shared_connection) => shared_connection.store_iter(metrics),
            Self::Batched { .. } | Self::Delayed { .. } => unreachable!(),
        }
    }
}

impl TestMetrics {
    pub fn failed() -> Self {
        Self {
            runtime: 0,
            conflict_literals: 0,
            propagations: 0,
            number_of_clauses: 0,
            number_of_variables: 0,
            parse_time: 0,
            restarts: 0,
            satisfiable: Satisfiability::Unknown,
            conflicts: 0,
            memory_usage: 0,
        }
    }
}
