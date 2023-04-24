use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::sync::{Arc, Mutex};

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
pub struct Test {
    pub id: i32,
    pub runtime: u64,
    pub satisfiable: Satisfiability,
    pub test_set: i64,
    pub benchmark: i64,
    pub solver: i64,
    pub target: String,
}

// TODO: Document below, maybe add some kind of migration utility
// ref: https://duckdb.org/docs/sql/statements/create_table.html
//      https://duckdb.org/docs/sql/data_types/overview
pub const SQL_SCHEMA: [&'static str; 8] = [
    "create sequence if not exists seq_benchmarks start 1 no cycle;",
    "create table if not exists benchmarks (
    id integer primary key default(nextval('seq_benchmarks')),
    comment varchar
);",
    "create sequence if not exists seq_testset start 1 no cycle;",
    "create table if not exists test_sets (
    id integer primary key default(nextval('seq_testset')),
    timout uinteger not null,
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
    satisfiable tinyint not null,
    target string not null,
    solver integer not null references solvers (id),
    test integer not null references test_sets (id),
    benchmark integer not null references benchmarks (id),
);",
];
pub const SQL_SCHEMA_NUMBER: usize = SQL_SCHEMA.len();

pub type Connection = Arc<Mutex<duckdb::Connection>>;
