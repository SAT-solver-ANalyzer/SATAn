mod collector;
mod config;
mod database;
mod executors;
mod ingest;

#[cfg(feature = "distributed")]
mod distributed;

use clap::{crate_name, crate_version, Args, Parser, Subcommand};
use config::ConfigErrors;
use std::{collections::BTreeMap, fs::File, io::BufReader, path::PathBuf, process::exit};
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct CLI {
    #[arg(short = 'b', long = "benchmark", help = "benchmark to continue")]
    benchmark: Option<i32>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone, Debug)]
enum Commands {
    /// Merge multiple SATAn runner metric databases
    Merge(MergeArgs),
    /// Execute a benchmark suite
    Execute(ExecuteArgs),
}

#[derive(Clone, Debug, Args)]
pub struct ExecuteArgs {
    #[arg(
        short = 'c',
        long = "config",
        value_name = "CONFIG",
        value_hint = clap::ValueHint::FilePath,
        help = "Path to the config file"
        )]
    config: PathBuf,
    #[arg(
        long = "comment",
        value_name = "COMMENT",
        help = "Comment that should be added to the benchmark"
    )]
    comment: Option<String>,
    #[arg(
        short = 's',
        long = "solver",
        value_name = "SOLVER",
        help = "solver that should be used in benchmark (default: all)"
    )]
    solvers: Option<Vec<String>>,
    #[arg(
        short = 't',
        long = "test",
        value_name = "TEST",
        help = "test set that should be used in benchmark (default: all)"
    )]
    tests: Option<Vec<String>>,
}

#[derive(Clone, Debug, Args)]
pub struct MergeArgs {
    #[arg(short = 'd', long = "databases", help = "databases to merge")]
    databases: Vec<PathBuf>,
}

fn main() -> Result<(), ConfigErrors> {
    // give a small info as a disclaimer for development progress
    info!("{} {} - pre ALPHA", crate_name!(), crate_version!());

    // parse the args with clap
    let args = CLI::parse();

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

    match args.command {
        Commands::Merge { .. } => todo!(),
        Commands::Execute(sub_args) => {
            // determine if the solver follows the correct syntax, exists ...
            let mut config: config::SolverConfig = if sub_args.config.is_file() {
                match File::open(sub_args.config).map(BufReader::new) {
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
                    sub_args.config.to_string_lossy()
                );

                exit(1);
            };

            // pre filte config solvers and test sets
            if let Some(solvers) = sub_args.solvers {
                config.solvers = config
                    .solvers
                    .iter()
                    .filter(|(key, _)| !solvers.contains(&key.to_string()))
                    .fold(BTreeMap::new(), |mut acc, (key, value)| {
                        acc.insert(key.clone(), value.clone());

                        acc
                    });
            }

            if let Some(test_set) = sub_args.tests {
                config.tests = config
                    .tests
                    .iter()
                    .filter(|(key, _)| !test_set.contains(&key.to_string()))
                    .fold(BTreeMap::new(), |mut acc, (key, value)| {
                        acc.insert(key.clone(), value.clone());

                        acc
                    });
            }

            // Check semantic structure (solver references, etc.)
            if config.preflight_checks() {
                error!("Config contains one or more errors, see previous error messages");

                exit(1);
            }

            debug!("Config: {config:?}");

            let mut connection = match database::ConnectionAdapter::load(&config.database) {
                Ok(connection) => connection,
                Err(error) => {
                    error!(error = ?error, "Failed to load connection: {error}");

                    exit(1)
                }
            };

            if let Err(error) = connection.init(&config, args.benchmark, sub_args.comment) {
                error!(error = ?error, "Failed to initialize the database connection: {error}");

                exit(1)
            };

            // TODO: Get rid of full clone here, this should be limited to config.ingest
            let cloned_config = config.clone();
            let ingestors = match cloned_config.load_ingestors() {
                Ok(ingestors) => ingestors,
                Err(error) => {
                    error!(
                        error = ?error,
                        "Preflight checks failed on ingestors: {error}"
                    );
                    exit(1)
                }
            };

            let collectors = match config.collectors() {
                Ok(collectors) => collectors,
                Err((name, error)) => {
                    error!(error = ?error, "Failed to compile collector for {name}: {error}");
                    exit(1);
                }
            };

            // select an executor and throw the queue at it
            match executors::LocalExecutor::load(connection, config, ingestors, collectors) {
                Ok(executor) => match executor.execute() {
                    Ok(()) => info!("Finished execution"),
                    Err(error) => error!(error = ?error, "Executor failed: {error}"),
                },
                Err(executor) => error!("Executor {executor} is not supported",),
            }

            Ok(())
        }
    }
}
