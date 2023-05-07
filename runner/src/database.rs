pub mod batched;
#[cfg(feature = "clickhouse")]
pub mod clickhouse;
pub mod duckdb;
#[cfg(feature = "rusqlite")]
pub mod sqlite;
pub mod util;

use std::{fmt::Debug, path::PathBuf};

use crate::config::{DatabaseConfig, SolverConfig};
use cowstr::CowStr;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use thiserror::Error;

use self::batched::BatchedConnection;

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

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("DuckDB adapter error")]
    DuckDB(duckdb::DuckDBError),
    #[cfg_attr(feature = "rusqlite", error("SQLite adapter error"))]
    #[cfg(feature = "rusqlite")]
    SQLite(rusqlite::Error),
    #[error("Invalid adapter configuration")]
    ConfigError,
}

#[derive(Debug)]
pub enum StorageAdapters {
    DuckDB(duckdb::SharedConnection),
    #[cfg(feature = "rusqlite")]
    SQLite(sqlite::SharedConnection),
    DuckDBBatched(batched::BatchedConnection),
    #[cfg(feature = "clickhouse")]
    ClickHouse(clickhouse::CHConnection),
}

/// Trait abstracting the interface to a database
impl StorageAdapters {
    /// Insert a single run output entry
    pub fn store(
        &self,
        metrics: TestMetrics,
        solver: CowStr,
        test_set: CowStr,
        target: &PathBuf,
    ) -> Result<ID, ConnectionError> {
        match self {
            Self::DuckDB(shared_connection) => {
                shared_connection.store(metrics, solver, test_set, target)
            }
            Self::DuckDBBatched(shared_connection) => {
                shared_connection.store(metrics, solver, test_set, target)
            }
            Self::SQLite(shared_connection) => {
                shared_connection.store(metrics, solver, test_set, target)
            }
            #[cfg(feature = "clickhouse")]
            Self::ClickHouse { .. } => todo!(),
        }
    }

    /// Close the connection and ensure consistency (i.e., finish batched inserts)
    pub fn close(self) -> Result<(), ConnectionError> {
        match self {
            Self::DuckDB(shared_connection) => shared_connection.close(),
            Self::SQLite(shared_connection) => shared_connection.close(),
            Self::DuckDBBatched(shared_connection) => shared_connection.close(),
            #[cfg(feature = "clickhouse")]
            Self::ClickHouse { .. } => todo!(),
        }
    }

    /// Estabilish a connection to the specified database
    pub fn load(config: &DatabaseConfig) -> Result<Self, ConnectionError> {
        match config {
            DatabaseConfig::DuckDB { .. } => {
                duckdb::SharedConnection::load(config).map(Self::DuckDB)
            }
            DatabaseConfig::SQLite { .. } => {
                sqlite::SharedConnection::load(config).map(Self::SQLite)
            }
            DatabaseConfig::Batched { .. } => {
                BatchedConnection::load(config).map(Self::DuckDBBatched)
            }
            #[cfg(feature = "clickhouse")]
            DatabaseConfig::ClickHouse { .. } => todo!(),
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
            Self::DuckDB(shared_connection) => shared_connection.init(config, benchmark, comment),
            Self::DuckDBBatched(shared_connection) => {
                shared_connection.init(config, benchmark, comment)
            }
            Self::SQLite(shared_connection) => shared_connection.init(config, benchmark, comment),
            #[cfg(feature = "clickhouse")]
            Self::ClickHouse { .. } => todo!(),
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
