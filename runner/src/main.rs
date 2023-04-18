mod config;
mod executors;
use crate::executors::Executor;
use std::{fs::File, io::BufReader, process::exit};

use clap::Parser;
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(short = 'c', long = "config", value_name = "CONFIG", value_hint = clap::ValueHint::FilePath)]
    config: std::path::PathBuf,
}

fn main() {
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

    // select an executor and throw the queue at it
    match executors::Executors::load(config) {
        Ok(mut executor) => match executor.execute() {
            Ok(()) => info!("Finished execution"),
            Err(e) => error!("Executor failed: {e:?}"),
        },
        Err(executor) => error!(
            "Executor {executor:?} is not supported, please see the documentation for supported options",
        ),
    }
}
