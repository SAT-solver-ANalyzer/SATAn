mod collector;
mod config;
mod database;
mod executors;
mod ingest;

#[cfg(feature = "distributed")]
mod distributed;
#[cfg(feature = "distributed")]
use distributed::{
    fs::{DONE_PREFIX, PROCESSING_PREFIX},
    SynchronizationTypes,
};
use tracing_unwrap::ResultExt;

use crate::{
    collector::Collector,
    config::ExecutorConfig,
    distributed::util::{rename, strip_prefix},
};
use clap::{crate_name, crate_version, Args, Parser, Subcommand};
use config::ConfigErrors;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
    process::exit,
};
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct CLI {
    #[arg(short = 'b', long = "benchmark", help = "benchmark to continue")]
    benchmark: Option<i32>,

    #[arg(
        short = 'c',
        long = "config",
        value_name = "CONFIG",
        value_hint = clap::ValueHint::FilePath,
        help = "Path to the config file",
        default_value = "solvers.yml",
        )]
    config: PathBuf,

    #[cfg(feature = "tracing")]
    #[arg(
        short = 't',
        long = "opentelemetry",
        help = "Enable opentelemetry tracing subscriber"
    )]
    opentelemetry: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone, Debug)]
enum Commands {
    /// Merge multiple SATAn runner metric databases
    Merge(MergeArgs),
    /// Removeall processing and done prefixes from test files
    Clean,
    /// Execute a benchmark suite
    Execute(ExecuteArgs),
}

#[derive(Clone, Debug, Args)]
pub struct ExecuteArgs {
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

fn setup_global_subscriber(cli: &CLI) {
    // Configure a custom event formatter and registry
    let registry = tracing_subscriber::registry()
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
        );

    #[cfg(feature = "tracing")]
    {
        let tracer = match opentelemetry_jaeger::new_agent_pipeline()
            .with_service_name("satan")
            .install_simple()
        {
            Ok(tracer) => tracer,
            Err(error) => {
                error!(error = ?error, "Failed to connect jeager tracing backend: {error}");

                exit(1);
            }
        };

        if cli.opentelemetry {
            let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);
            registry.with(opentelemetry).init();
        } else {
            registry.init();
        }
    }
    #[cfg(not(feature = "tracing"))]
    registry.init()
}

fn main() -> Result<(), ConfigErrors> {
    // give a small info as a disclaimer for development progress
    info!("{} {} - pre ALPHA", crate_name!(), crate_version!());

    // parse the args with clap
    let args = CLI::parse();
    setup_global_subscriber(&args);

    debug!("Args: {args:?}");

    match args.command {
        Commands::Merge { .. } => todo!(),
        Commands::Clean => {
            // determine if the solver follows the correct syntax, exists ...
            let mut config: config::SolverConfig = config::SolverConfig::load(&args.config);

            // Check semantic structure (solver references, etc.)
            if config.preflight_checks() {
                error!("Config contains one or more errors, see previous error messages");

                exit(1);
            }

            let collectors = match config.collectors() {
                Ok(collectors) => collectors,
                Err((name, error)) => {
                    error!(error = ?error, name = %name, "Failed to compile collector for {name}: {error}");
                    exit(1);
                }
            };

            collectors.into_iter().for_each(|(name, collector)| {
                info!(name = %name, "Handling new collector");

                // iterate over files, match on file_name prefix
                for file in collector {
                    if let Some(file_name) = file.file_name().map(OsStr::to_os_string) {
                        if file_name.len() > DONE_PREFIX.len()
                            && file_name.as_bytes()[..DONE_PREFIX.len()] == *DONE_PREFIX.as_bytes()
                        {
                            info!(file_name = ?file_name, "Rename filename");

                            let mut new_file_name =
                                OsString::with_capacity(file_name.len() - DONE_PREFIX.len());
                            new_file_name =
                                strip_prefix(file_name, DONE_PREFIX.clone(), new_file_name);
                            let mut new_file_path = file.clone().to_path_buf();
                            new_file_path.set_file_name(new_file_name);

                            rename(&file, &new_file_path).unwrap_or_log();
                        } else if file_name.len() > PROCESSING_PREFIX.len()
                            && file_name.as_bytes()[..PROCESSING_PREFIX.len()]
                                == *PROCESSING_PREFIX.as_bytes()
                        {
                            info!(file_name = ?file_name, "Rename filename");

                            let mut new_file_name =
                                OsString::with_capacity(file_name.len() - PROCESSING_PREFIX.len());
                            new_file_name =
                                strip_prefix(file_name, PROCESSING_PREFIX.clone(), new_file_name);
                            let mut new_file_path = file.clone().to_path_buf();
                            new_file_path.set_file_name(new_file_name);

                            rename(&file, &new_file_path).unwrap_or_log();
                        }
                    }
                }
            });

            Ok(())
        }
        Commands::Execute(sub_args) => {
            // determine if the solver follows the correct syntax, exists ...
            let mut config: config::SolverConfig = config::SolverConfig::load(&args.config);

            // pre filter config solvers and test sets
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

            // TODO: Inject MPICollector and FS Collector
            let collectors = match config.collectors() {
                Ok(mut collectors) => match config.executor {
                    ExecutorConfig::Distributed {
                        ref synchronization,
                    } => {
                        match synchronization {
                            SynchronizationTypes::Coordinated => {
                                // TODO: Create coordinator here and share across collectors
                                collectors.iter_mut().for_each(|(_, value)| {
                                    *value = Collector::mpi(value.clone());
                                });
                            }
                            SynchronizationTypes::FileSystem { .. } => {
                                collectors.iter_mut().for_each(|(_, value)| {
                                    *value = Collector::fs(value.clone());
                                });
                            }
                        };

                        collectors
                    }
                    _ => collectors,
                },
                Err((name, error)) => {
                    error!(error = ?error, name = %name, "Failed to compile collector for {name}: {error}");
                    exit(1);
                }
            };

            // select an executor ...
            #[cfg(feature = "distributed")]
            let executor = match config.executor {
                ExecutorConfig::Distributed {
                    synchronization: SynchronizationTypes::Coordinated,
                } => todo!(),
                ExecutorConfig::Distributed {
                    synchronization: SynchronizationTypes::FileSystem { .. },
                }
                | ExecutorConfig::Local { .. } => {
                    executors::LocalExecutor::load(connection, config, ingestors, collectors)
                }
            };

            #[cfg(not(feature = "distributed"))]
            let executor =
                executors::LocalExecutor::load(connection, config, ingestors, collectors);

            // ... and throw the queue at it
            match executor {
                Ok(executor) => match executor.execute() {
                    Ok(()) => info!("Finished execution"),
                    Err(error) => error!(error = ?error, "Executor failed: {error}"),
                },
                Err(error) => error!(error = ?error, "Executor failed to initialize"),
            }

            Ok(())
        }
    }
}
