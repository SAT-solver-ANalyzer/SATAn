pub mod util;

use duckdb::params;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::sync::{Arc, Mutex};

use crate::ingest::RunContext;

// TODO: Factor out duckdb adapter -> wrap conn in enum
// MID TERM: add clickhouse
// LONG TERM: add exporter (CSV) and migrator (duckdb <-> clickhouse)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Benchmark {
    pub id: i32,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSet {
    pub name: String,
    pub id: i64,
    pub params: Option<String>,
    pub timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solver {
    pub id: i32,
    pub name: String,
    pub exec: String,
    pub params: Option<String>,
    pub ingest: String,
}

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

impl TestMetrics {
    pub fn failed(runtime: u64) -> Self {
        Self {
            runtime,
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

    pub fn insert(
        self,
        tx: &mut duckdb::Transaction,
        context: RunContext,
    ) -> Result<i32, duckdb::Error> {
        let mut stmt = tx.prepare_cached(
            "insert into runs values
                (nextval('seq_run_id'), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                returning id",
        )?;

        stmt.query_row(
            params![
                self.runtime,
                self.parse_time,
                self.satisfiable as i8,
                self.memory_usage,
                self.restarts,
                self.conflicts,
                self.propagations,
                self.conflict_literals,
                self.number_of_variables,
                self.number_of_clauses,
                context.path.to_string_lossy().as_ref(),
                context.solver.0,
                context.testset.0,
                context.benchmark
            ],
            |row| row.get(0),
        )
    }
}

// TODO: Document below, maybe add some kind of migration utility
// ref: https://duckdb.org/docs/sql/statements/create_table.html
//      https://duckdb.org/docs/sql/data_types/overview
pub const SQL_SCHEMA: [&str; 8] = [
    "create sequence if not exists seq_benchmarks start 1 no cycle;",
    "create table if not exists benchmarks (
    id integer primary key default(nextval('seq_benchmarks')),
    comment varchar
);",
    "create sequence if not exists seq_testset start 1 no cycle;",
    "create table if not exists test_sets (
    id integer primary key default(nextval('seq_testset')),
    timeout uinteger not null,
    name varchar not null,
    params varchar not null,
);",
    "create sequence if not exists seq_solver_id start 1 no cycle;",
    "create table if not exists solvers (
    id integer primary key default(nextval('seq_solver_id')),
    name varchar not null,
    exec varchar not null,
    params varchar not null,
    ingest varchar not null
);",
    "create sequence if not exists seq_run_id start 1 no cycle;",
    "create table if not exists runs (
    id integer primary key default(nextval('seq_run_id')),

    runtime ubigint not null,
    parse_time ubigint not null,
    satisfiable tinyint not null,
    memory_usage uinteger not null,
    restarts uinteger not null,
    conflicts uinteger not null,
    propagations uinteger not null,
   
	conflict_literals uinteger not null,
	number_of_variables uinteger not null,
    number_of_clauses uinteger not null,
    target string not null,

    solver integer not null references solvers (id),
    test integer not null references test_sets (id),
    benchmark integer not null references benchmarks (id)
);",
];
pub const SQL_SCHEMA_NUMBER: usize = SQL_SCHEMA.len();

pub type Connection = Arc<Mutex<duckdb::Connection>>;
