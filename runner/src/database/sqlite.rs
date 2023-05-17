use super::{util::IDMap, MetricsBundle, TestMetrics, ID};
use crate::{
    config::{ConnectionConfig, SolverConfig},
    database::ConnectionError,
};
use cowstr::CowStr;
use parking_lot::{lock_api::ArcMutexGuard, FairMutex, RawFairMutex};
use rusqlite::{params, Connection};
use std::{fmt::Debug, iter::Iterator, path::PathBuf, sync::Arc};
use tracing::{debug, error, info};
use tracing_unwrap::ResultExt;

#[derive(Debug)]
/// Transparent, thread safe wrapper over `InnerConnection`
pub struct SharedConnection(Arc<FairMutex<InnerConnection>>);

#[derive(Debug)]
pub struct InnerConnection {
    connection: Connection,
    solvers: IDMap,
    test_sets: IDMap,
    benchmark: i32,
}

impl From<rusqlite::Error> for ConnectionError {
    fn from(error: rusqlite::Error) -> Self {
        ConnectionError::SQLite(error)
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
                Err(error) => {
                    error!(error = ?error, table = table, "Failed to apply SQL schema ({counter}/{SQL_SCHEMA_NUMBER}): {error}");

                    return Err(ConnectionError::SQLite(error));
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

                    info!(
                        name = %name,
                        id = %id,
                        "Was able to reuse existing solver entry"
                    );
                    self.solvers.insert(name.clone(), id);

                    break;
                }
            }

            if !found_result {
                let id = self
                    .connection
                    .prepare_cached(
                        "insert into solvers
                         (name, exec, params, ingest)
                         values (?, ?, ?, ?) returning id",
                    )?
                    .query_row(
                        params![
                            name.as_str(),
                            solver.exec.to_string_lossy(),
                            solver.params.join(" "),
                            solver.ingest.as_str()
                        ],
                        |row| row.get(0),
                    )?;

                info!(name = %name, id = %id, "Created solver entry");
                self.solvers.insert(name.clone(), id);
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

                    info!(name = %name, id = %id, "Was able to reuse existing test set entry");
                    self.test_sets.insert(name.clone(), id);

                    break;
                }
            }

            if !found_result {
                let id = self
                    .connection
                    .prepare_cached(
                        "insert into test_sets 
                         (timeout, name, params) values (?, ?, ?)
                         returning id",
                    )?
                    .query_row(params![set.timeout, name.as_str(), current_params], |row| {
                        row.get(0)
                    })?;

                info!(name = %name, id = %id, "Created set entry");
                self.test_sets.insert(name.clone(), id);
            }
        }

        Ok(())
    }

    fn new_benchmark(&mut self, comment: Option<String>) -> Result<i32, ConnectionError> {
        let id = self
            .connection
            .prepare_cached(
                "insert into benchmarks
                 (comment) values (?)
                 returning id",
            )?
            .query_row(params![comment.unwrap_or("".to_owned())], |row| row.get(0))?;

        info!(id = id, "Created new benchmark");

        Ok(id)
    }

    pub fn close(mut self) -> Result<(), ConnectionError> {
        let mut counter = 0;
        while let Err((connection, error)) = self.connection.close() {
            counter += 1;
            self.connection = connection;
            error!(error = ?error, "Failed to close SQLite connection: {error}, trying again {counter}/3");

            if counter == 3 {
                error!("Failed to close connection, SOL");

                return Err(ConnectionError::SQLite(error));
            }
        }

        info!("Closed SQLite connection");

        Ok(())
    }

    pub fn load(config: &ConnectionConfig) -> Result<Self, ConnectionError> {
        match config {
            ConnectionConfig::SQLite { path } => {
                let connection = Connection::open(path)?;

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

        self.connection
            .prepare_cached(
                "insert into runs
                (runtime, parse_time, satisfiable, memory_usage, restarts, conflicts,
                 propagations, conflict_literals, number_of_variables,
                 number_of_clauses, target, solver, test, benchmark)
                values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) returning id",
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
            .map_err(ConnectionError::SQLite)
    }

    pub fn store_iter<'a, I: Iterator<Item = MetricsBundle>>(
        &self,
        mut metrics: I,
    ) -> Result<(), ConnectionError> {
        let mut counter = 0;

        // NOTE: We can guarantee that no nested transactions are present due to only having one
        // connection at a time.
        let mut tx = self.connection.unchecked_transaction()?;
        tx.set_drop_behavior(rusqlite::DropBehavior::Rollback);
        metrics.try_for_each(|bundle| -> Result<(), ConnectionError> {
            counter += 1;
            let id: i32 = tx
                .prepare_cached(
                    "insert into runs
                (runtime, parse_time, satisfiable, memory_usage, restarts, conflicts,
                 propagations, conflict_literals, number_of_variables,
                 number_of_clauses, target, solver, test, benchmark)
                values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                returning id",
                )?
                .query_row(
                    params![
                        bundle.metrics.runtime,
                        bundle.metrics.parse_time,
                        bundle.metrics.satisfiable.clone() as i8,
                        bundle.metrics.memory_usage,
                        bundle.metrics.restarts,
                        bundle.metrics.conflicts,
                        bundle.metrics.propagations,
                        bundle.metrics.conflict_literals,
                        bundle.metrics.number_of_variables,
                        bundle.metrics.number_of_clauses,
                        bundle.target.to_string_lossy().as_ref(),
                        self.solvers.get(&bundle.solver).unwrap(),
                        self.test_sets.get(&bundle.test_set).unwrap(),
                        self.benchmark
                    ],
                    |row| row.get(0),
                )?;

            debug!(id = id, "Inserted entry");

            Ok(())
        })?;
        tx.commit()?;

        info!("Stored {counter} entries");

        Ok(())
    }
}

// TODO: Document below, maybe add some kind of migration utility
// ref: https://duckdb.org/docs/sql/statements/create_table.html
//      https://duckdb.org/docs/sql/data_types/overview
pub const SQL_SCHEMA: [&str; 4] = [
    "create table if not exists benchmarks (
    id integer primary key,
    comment text
);",
    "create table if not exists test_sets (
    id integer primary key,
    timeout uinteger not null,
    name text not null,
    params text not null
);",
    "create table if not exists solvers (
    id integer primary key,
    name text not null,
    exec text not null,
    params text not null,
    ingest text not null
);",
    "create table if not exists runs (
    id integer primary key,

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
