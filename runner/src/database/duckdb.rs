use super::{util::IDMap, MetricsBundle, TestMetrics, ID};
use crate::{
    config::{ConnectionConfig, SolverConfig},
    database::ConnectionError,
};
use cowstr::CowStr;
use duckdb::params;
use parking_lot::{lock_api::ArcMutexGuard, FairMutex, RawFairMutex};
use std::{fmt::Debug, iter::Iterator, path::PathBuf, sync::Arc};
use thiserror::Error;
use tracing::{debug, error, info, trace};
use tracing_unwrap::ResultExt;

#[derive(Debug)]
/// Transparent, thread safe wrapper over `InnerConnection`
pub struct SharedConnection(Arc<FairMutex<InnerConnection>>);

#[derive(Debug)]
pub struct InnerConnection {
    connection: duckdb::Connection,
    solvers: IDMap,
    test_sets: IDMap,
    benchmark: i32,
}

#[derive(Error, Debug)]
pub enum DuckDBError {
    #[error("DuckDB Error")]
    DuckDB(#[from] duckdb::Error),
}

impl From<duckdb::Error> for ConnectionError {
    fn from(value: duckdb::Error) -> Self {
        ConnectionError::DuckDB(DuckDBError::DuckDB(value))
    }
}

impl SharedConnection {
    pub fn new(inner_connection: InnerConnection) -> Self {
        Self(Arc::new(FairMutex::new(inner_connection)))
    }

    fn lock_mut(&mut self) -> ArcMutexGuard<RawFairMutex, InnerConnection> {
        self.0.lock_arc()
    }

    fn lock(&self) -> ArcMutexGuard<RawFairMutex, InnerConnection> {
        self.0.lock_arc()
    }

    pub fn init(
        &mut self,
        config: &SolverConfig,
        benchmark: Option<ID>,
        comment: Option<String>,
    ) -> Result<(), ConnectionError> {
        self.lock_mut().init(config, benchmark, comment)
    }

    pub fn close(self) -> Result<(), ConnectionError> {
        Arc::try_unwrap(self.0).unwrap_or_log().into_inner().close()
    }

    pub fn load(config: &ConnectionConfig) -> Result<Self, ConnectionError> {
        Ok(Self::new(InnerConnection::load(config)?))
    }

    pub fn store(
        &self,
        metrics: TestMetrics,
        solver: CowStr,
        test_set: CowStr,
        target: &PathBuf,
    ) -> Result<i32, ConnectionError> {
        self.lock().store(metrics, solver, test_set, target)
    }

    pub fn store_iter<'a, I: Iterator<Item = MetricsBundle>>(
        &self,
        metrics: I,
    ) -> Result<(), ConnectionError> {
        self.lock().store_iter(metrics)
    }
}

// TODO: [ ] Use DuckDB properly, i.e., use batching with an appender
// TODO: [x] Check if tables present, create if not
// TODO: [x] Iterate over solvers, check if present, if not create
// TODO: [ ] |- Collect solver ids into hashmap
// TODO: [ ] Iterate over test set, check if present, if not create
// TODO: [ ] |- Collect test set ids into hashmap
// TODO: [ ] Create new Benchmark -> crate Arc over uuid for Benchmark
// TODO: [ ] Create Arc<Mutex<Connection>> and test performance otherwise implement buffered writer
// with channels

impl InnerConnection {
    pub fn init(
        &mut self,
        config: &SolverConfig,
        benchmark: Option<ID>,
        comment: Option<String>,
    ) -> Result<(), ConnectionError> {
        let mut counter = 1;

        for table in SQL_SCHEMA {
            match self.connection.execute(table, []) {
                Ok(_) => info!("Applied SQL schema ({counter}/{SQL_SCHEMA_NUMBER})"),
                Err(e) => {
                    error!("Failed to apply SQL schema ({counter}/{SQL_SCHEMA_NUMBER}): {e}",);
                    trace!("schema: {table}");

                    return Err(ConnectionError::DuckDB(DuckDBError::DuckDB(e)));
                }
            };

            counter += 1;
        }

        if let Some(benchmark_id) = benchmark {
            self.benchmark = benchmark_id;

            // TODO: Check if the comment should be updated
        } else {
            self.benchmark = self.new_benchmark(comment)?;
        };

        // pre-register all solvers and test sets in database
        for (name, solver) in config.solvers.iter() {
            let results = self
                .connection
                .prepare_cached("select id, exec, params, ingest from solvers where name = ?")?
                .query_map(params![name.as_str()], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .try_fold(Vec::new(), |mut init, result| {
                    init.push(result?);

                    Ok::<Vec<(i32, String, String, String)>, ConnectionError>(init)
                })?;

            // check if either no solver with the name is found or none with their parameters exists
            let current_params = solver.get_params();
            let current_exec = solver.exec.to_string_lossy();

            let mut found_result = false;

            for (id, exec, params, ingest) in results {
                if current_exec == exec
                    && ingest == solver.ingest.to_string()
                    && params == current_params
                {
                    found_result = true;

                    info!("Was able to reuse existing solver entry for {name}, id: {id}",);
                    self.solvers.insert(name.clone(), id);
                    break;
                }
            }

            if !found_result {
                let tx = self.connection.transaction()?;

                let id =tx.query_row(
                "insert into solvers values (nextval('seq_solver_id'), ?, ?, ?, ?) returning id",
                params![
                    name.as_str(),
                    solver.exec.to_string_lossy(),
                    solver.params.join(" "),
                    solver.ingest.as_str()
                ],
                |row| row.get(0),
            )?;

                info!("Created solver entry for {name}, id: {id}");
                self.solvers.insert(name.clone(), id);

                tx.commit()?;
            }
        }

        for (name, set) in config.tests.iter() {
            let results = self
                .connection
                .prepare_cached("select id, timeout, params from test_sets where name = ?")?
                .query_map(params![name.as_str()], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .try_fold(Vec::new(), |mut init, result| {
                    init.push(result?);

                    Ok::<Vec<(i32, u32, String)>, ConnectionError>(init)
                })?;

            // check if either no solver with the name is found or none with their parameters exists
            let current_params = set.get_params();

            let mut found_result = false;

            for (id, timeout, params) in results {
                if timeout == set.timeout && params == current_params {
                    found_result = true;

                    info!(
                        name = %name,
                        id = %id,
                        "Was able to reuse existing test set entry",
                    );
                    self.test_sets.insert(name.clone(), id);
                    break;
                }
            }

            if !found_result {
                let tx = self.connection.transaction()?;

                let id = tx.query_row(
                    "insert into test_sets values (nextval('seq_testset'), ?, ?, ?) returning id",
                    params![set.timeout, name.as_str(), current_params],
                    |row| row.get(0),
                )?;

                info!("Created set entry for {name}, id: {id}");
                self.test_sets.insert(name.clone(), id);

                tx.commit()?;
            }
        }

        Ok(())
    }

    fn new_benchmark(&mut self, comment: Option<String>) -> Result<i32, ConnectionError> {
        let tx = self.connection.transaction()?;

        let id = tx.query_row(
            "insert into benchmarks values (nextval('seq_benchmarks'), ?) returning id",
            params![comment.unwrap_or("".to_owned())],
            |row| row.get(0),
        )?;

        info!("Created new benchmark - id: {id}");

        tx.commit()?;

        Ok(id)
    }

    pub fn close(mut self) -> Result<(), ConnectionError> {
        let mut counter = 0;
        while let Err((connection, error)) = self.connection.close() {
            counter += 1;
            self.connection = connection;
            error!(error = ?error, "Failed to close duckdb connection: {error}, trying again {counter}/3");

            if counter == 3 {
                error!("Failed to close connection, SOL");

                return Err(ConnectionError::DuckDB(DuckDBError::DuckDB(error)));
            }
        }

        info!("Closed DuckDB connection");

        Ok(())
    }

    pub fn load(config: &ConnectionConfig) -> Result<Self, ConnectionError> {
        match config {
            ConnectionConfig::DuckDB { path } => {
                let connection = duckdb::Connection::open(path)?;

                Ok(Self {
                    connection,
                    solvers: IDMap::new(),
                    test_sets: IDMap::new(),
                    benchmark: ID::MIN,
                })
            }
            _ => unreachable!(),
        }
    }

    pub fn store(
        &self,
        metrics: TestMetrics,
        solver: CowStr,
        test_set: CowStr,
        target: &PathBuf,
    ) -> Result<i32, ConnectionError> {
        debug!("Inserting {metrics:?}...");

        let result = self
            .connection
            .prepare_cached(
                "insert into runs values
                (nextval('seq_run_id'), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                returning id",
            )?
            .query_row(
                params![
                    if metrics.runtime == 0 {
                        None
                    } else {
                        Some(metrics.runtime)
                    },
                    metrics.parse_time,
                    metrics.satisfiable as i8,
                    metrics.memory_usage,
                    metrics.restarts,
                    metrics.conflicts,
                    metrics.propagations,
                    metrics.conflict_literals,
                    metrics.number_of_variables,
                    metrics.number_of_clauses,
                    target.to_string_lossy().as_ref(),
                    self.solvers.get(&solver).unwrap(),
                    self.test_sets.get(&test_set).unwrap(),
                    self.benchmark
                ],
                |row| row.get(0),
            )
            .map_err(|err| ConnectionError::DuckDB(DuckDBError::DuckDB(err)));

        match result {
            Ok(result) => Ok(result),
            Err(err) => Err(err),
        }
    }

    pub fn store_iter<'a, I: Iterator<Item = MetricsBundle>>(
        &mut self,
        mut metrics: I,
    ) -> Result<(), ConnectionError> {
        let tx = self.connection.transaction()?;
        let mut appender = tx.appender("runs")?;
        let mut counter = 0;

        metrics.try_for_each(|bundle| -> Result<(), ConnectionError> {
            counter += 1;
            appender
                .append_row(params![
                    "nextval('seq_run_id')",
                    bundle.metrics.runtime,
                    bundle.metrics.parse_time,
                    bundle.metrics.satisfiable.clone() as i8,
                    bundle.metrics.memory_usage,
                    bundle.metrics.restarts,
                    bundle.metrics.conflicts,
                    bundle.metrics.propagations,
                    bundle.metrics.number_of_variables,
                    bundle.metrics.number_of_clauses,
                    bundle.target.to_string_lossy().as_ref(),
                    self.solvers.get(&bundle.solver).unwrap(),
                    self.test_sets.get(&bundle.test_set).unwrap(),
                    self.benchmark
                ])
                .map_err(|err| ConnectionError::from(err))
        })?;

        drop(appender);
        tx.commit()?;

        info!("Stored {counter} entries");

        Ok(())
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

    runtime ubigint,
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
