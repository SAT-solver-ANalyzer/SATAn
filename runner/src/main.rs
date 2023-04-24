mod config;
mod database;
mod executors;
mod ingest;

use crate::database::{util::retrieve_ids, SQL_SCHEMA, SQL_SCHEMA_NUMBER};
use clap::{crate_name, crate_version, Parser};
use config::ConfigErrors;
use duckdb::{params, Connection};
use std::{
    fs::File,
    io::BufReader,
    path::PathBuf,
    process::exit,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, trace};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(short = 'c', long = "config", value_name = "CONFIG", value_hint = clap::ValueHint::FilePath)]
    config: PathBuf,
    #[arg(long = "comment", value_name = "COMMENT")]
    comment: Option<String>, // TODO: Add args for selecting solvers and test sets
}

fn main() -> Result<(), ConfigErrors> {
    // give a small info as a disclaimer for development progress
    info!("{} {} - pre ALPHA", crate_name!(), crate_version!());

    // parse the args with clap
    let args = Args::parse();

    // Configure a custom event formatter and registry
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("info"))
                .unwrap(),
        )
        .with(
            fmt::layer()
                // required for good rayon debugging
                .with_thread_ids(true)
                .with_thread_names(false)
                .compact(),
        )
        .init();

    debug!("Args: {args:?}");

    // determine if the solver follows the correct syntax, exists ...
    let mut config: config::SolverConfig = if args.config.is_file() {
        match File::open(args.config).map(BufReader::new) {
            Ok(config_reader) => match serde_yaml::from_reader(config_reader) {
                Ok(config) => config,
                Err(e) => {
                    error!("Failed to deserialize config file: {e}");

                    exit(1);
                }
            },
            Err(e) => {
                error!("Couldn't open reader on config file: {e}");

                exit(1);
            }
        }
    } else {
        error!(
            "{} is not a file or doesn't exist, please provide an existing config file",
            args.config.to_string_lossy()
        );

        exit(1);
    };

    // Check semantic structure (solver references, etc.)
    if config.preflight_checks() {
        error!("Config contains one or more errors, see previous error messages");

        exit(1);
    }

    debug!("Config: {config:?}");

    let mut connection = match Connection::open(config.database.path.clone()) {
        Ok(connection) => connection,
        Err(e) => {
            error!("Failed to estabilish a connection for the metrics database: {e}");

            exit(1)
        }
    };

    // TODO: Add a migration system or something similiar here
    let mut counter = 1;
    for table in SQL_SCHEMA {
        match connection.execute(table, []) {
            Ok(_) => info!("Applied SQL schema ({counter}/{SQL_SCHEMA_NUMBER})"),
            Err(e) => {
                error!("Failed to apply SQL schema ({counter}/{SQL_SCHEMA_NUMBER}): {e}",);
                trace!("schema: {table}");
                exit(1)
            }
        };

        counter += 1;
    }

    // TODO: [x] Check if tables present, create if not
    // TODO: [x] Iterate over solvers, check if present, if not create
    // TODO: [ ] |- Collect solver ids into hashmap
    // TODO: [ ] Iterate over test set, check if present, if not create
    // TODO: [ ] |- Collect test set ids into hashmap
    // TODO: [ ] Create new Benchmark -> crate Arc over uuid for Benchmark
    // TODO: [ ] Create Arc<Mutex<Connection>> and test performance otherwise implement buffered writer
    // with channels

    // pre-register all solvers and test sets in database
    for (name, solver) in config.solvers.iter() {
        let results = connection
            .prepare_cached("select id, exec, params, ingest from solvers where name = ?")?
            .query_map(params![name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .try_fold(Vec::new(), |mut init, result| {
                init.push(result?);

                Ok::<Vec<(i32, String, String, String)>, ConfigErrors>(init)
            })?;

        let tx = connection.transaction()?;

        // check if either no solver with the name is found or none with their parameters exists
        if results.len() == 0
            || !results.iter().all(|result| {
                result.1 != solver.exec.to_string_lossy()
                    || result.2 != solver.params.join(" ")
                    || result.3 != solver.ingest
            })
        {
            tx.execute(
                "insert into solvers values (nextval('seq_solver_id'), ?, ?, ?, ?)",
                params![
                    name,
                    solver.exec.to_string_lossy(),
                    solver.params.join(" "),
                    solver.ingest
                ],
            )?;
            info!("Created solver entry for {name}")
        }

        tx.commit()?;
    }

    for (name, set) in config.tests.iter() {
        /*
           id integer primary key default(nextval('seq_testset')),
           timout uinteger not null,
           name varchar not null,
           params varchar not null,
        * */

        let results = connection
            .prepare_cached("select id, timeout, params from test_sets where name = ?")?
            .query_map(params![name], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .try_fold(Vec::new(), |mut init, result| {
                init.push(result?);

                Ok::<Vec<(i32, u32, String)>, ConfigErrors>(init)
            })?;

        let tx = connection.transaction()?;

        // check if either no solver with the name is found or none with their parameters exists
        let params = set
            .params
            .clone()
            .map(|params| params.join(" "))
            .unwrap_or("".to_owned());

        if results.len() == 0
            || !results
                .iter()
                .all(|result| (result.2 != params || result.1 != set.timeout as u32))
        {
            tx.execute(
                "insert into test_sets values (nextval('seq_testset'), ?, ?, ?)",
                params![set.timeout, name, params],
            )?;
            info!("Created solver entry for {name}");
        }

        tx.commit()?;
    }

    // collect solver and test set ids into a map for easier access during execution and ingestion
    let solver_ids = retrieve_ids(&connection, "select id, name from solvers")?;
    let testset_ids = retrieve_ids(&connection, "select id, name from test_sets")?;
    let benchmark_id = connection.query_row(
        "insert into benchmarks values (nextval('seq_benchmarks'), ?) returning id;",
        params![args.comment.unwrap_or("".to_owned())],
        |row| Ok(row.get(0)?),
    )?;

    // wrap the connection to allow for safe, concurrent access
    let shared_connection = Arc::new(Mutex::new(connection));

    // TODO: Get rid of full clone here, this should be limited to config.ingest
    let cloned_config = config.clone();
    let ingestors = match cloned_config.load_ingestors() {
        Ok(ingestors) => ingestors,
        Err(e) => {
            error!("Failed to initialize ingestors after preflight checks: {e}");
            exit(1)
        }
    };

    // select an executor and throw the queue at it
    match executors::Executors::load(shared_connection, config, ingestors, solver_ids, testset_ids, benchmark_id) {
        Ok(mut executor) => match executor.execute() {
            Ok(()) => info!("Finished execution"),
            Err(e) => error!("Executor failed: {e}"),
        },
        Err(executor) => error!(
            "Executor {executor} is not supported, please see the documentation for supported options",
        ),
    }

    Ok(())
}
