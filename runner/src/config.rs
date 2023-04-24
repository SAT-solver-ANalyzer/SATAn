use crate::{
    database::Connection,
    executors::ExecutorError,
    ingest::{IngestorMap, Ingestors},
};
use globset::{GlobBuilder, GlobMatcher};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap, fs::File, io::Error, os::unix::fs::MetadataExt, path::PathBuf,
    str::FromStr,
};
use thiserror::Error;
use tracing::{error, warn};

// check if a file is executable
pub fn check_executable(path: &PathBuf) -> Result<bool, ConfigErrors> {
    if !path.is_file() {
        Err(ConfigErrors::FileNotFound)
    } else {
        match File::open(path).map(|file| file.metadata()) {
            Ok(Ok(metadata)) => Ok((metadata.mode() & 0o111) != 0),
            Ok(Err(e)) | Err(e) => Err(ConfigErrors::MetadataNotFound(e)),
        }
    }
}

#[derive(Error, Debug)]
pub enum ConfigErrors {
    #[error("Globs were invalid")]
    InvalidGlobs(#[from] globset::Error),
    #[error("Executor not supported")]
    UnsupportedExecutor(String),
    #[error("Executor failed to load")]
    FailedLoadExecutor(#[from] ExecutorError),
    #[error("Ingestor failed to load")]
    FailedLoadIngestor,
    #[error("File not found")]
    FileNotFound,
    #[error("Metadata not found")]
    MetadataNotFound(#[from] Error),
    #[error("Database query failed")]
    DatabaseError(#[from] duckdb::Error),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SolverConfig {
    // executor config, has yet to be fully structured
    pub executor: ExecutorConfig,
    // Solvers as generic executables with fixed parameters, this might be extended later on
    pub solvers: BTreeMap<String, Solver>,
    // Tests as sets of test files, again only a stub for e.g., an interface of some kind
    pub tests: BTreeMap<String, TestSet>,
    // Config for all ingestor related setups
    pub ingest: BTreeMap<String, IngestorConfig>,

    #[serde(alias = "db")]
    pub database: DatabaseConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    #[serde(default = "default_database_path")]
    pub path: PathBuf,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct IngestorConfig {
    // Name of the selected ingestor type
    pub name: String,

    // parameters for the databse that apply over all tests
    // TODO: Make this fully typed with an enum
    #[serde(default)]
    pub parameter: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExecutorConfig {
    // Name of the selected executor, see Executors::from_str for the selection proccess
    pub name: String,
    // parameters for the executor that apply over all tests
    // TODO: Make this fully typed with an enum
    pub parameter: Option<BTreeMap<String, serde_yaml::Value>>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TestSet {
    pub timeout: usize,
    #[serde(default)]
    pub paths: Vec<String>,
    pub glob: String,
    #[serde(default)]
    pub solvers: Vec<String>,
    pub params: Option<Vec<String>>,
    pub path: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Solver {
    pub exec: PathBuf,
    #[serde(default)]
    pub params: Vec<String>,
    pub ingest: String,
    #[serde(default, skip)]
    pub _id: i32,
}

impl SolverConfig {
    /// load, if possible, all ingestors
    pub fn load_ingestors(&self, connection: Connection) -> Result<IngestorMap, ConfigErrors> {
        let mut ingestors = IngestorMap::new();
        let mut contains_error = false;

        for (name, config) in self.ingest.iter() {
            match Ingestors::load(config, connection.clone()) {
                Ok(ingestor) => {
                    ingestors.insert(name.clone(), ingestor);
                }
                Err(e) => {
                    error!("ingestor {name} failed to load: {e}");
                    contains_error = true;
                }
            };
        }

        if contains_error {
            Err(ConfigErrors::FailedLoadIngestor)
        } else {
            Ok(ingestors)
        }
    }

    /// Compile all globs for the test sets
    pub fn compile_globs(&mut self) -> Result<Vec<GlobMatcher>, Vec<(String, globset::Error)>> {
        let mut errors = Vec::new();
        let mut globs = Vec::new();

        self.tests.iter().for_each(|(name, test)| {
            match GlobBuilder::new(&test.glob)
                .build()
                .map(|glob| glob.compile_matcher())
            {
                Ok(matcher) => {
                    globs.push(matcher);
                }
                Err(error) => {
                    errors.push((name.clone(), error));
                }
            }
        });

        if errors.is_empty() {
            Ok(globs)
        } else {
            Err(errors)
        }
    }

    pub fn preflight_checks(&mut self) -> bool {
        // TODO: Below is not performant nor clean, it should only work as a band aid solution

        // attempt to catch all errors instead of piece-by-piece to make debugging easier for users
        let mut contains_error = false;

        if self.solvers.is_empty() {
            error!("No solver was defined, unable to build a queue of tests");
            contains_error = true;
        }

        for (name, config) in self.ingest.iter_mut() {
            config.name = config.name.to_lowercase();

            match config.name.as_str() {
                "raw" => {
                    if config
                        .parameter
                        .get("exec")
                        .filter(|value| value.is_string())
                        // TODO: Add proper error handling below
                        .filter(|value| {
                            check_executable(&PathBuf::from(value.as_str().unwrap())).unwrap()
                        })
                        .is_none()
                    {
                        error!("ingestor.{name}.parameter.exec must be a valid path to an executable file");
                        contains_error = true;
                    }
                }
                ingestor_name => {
                    error!("ingestor.{name}.name ({ingestor_name}) is not supported, please use `raw` for now");
                    contains_error = true;
                }
            }
        }
        let supported_ingestors = self.ingest.keys().sorted().cloned().collect_vec();

        for (name, solver) in self.solvers.iter() {
            if supported_ingestors.binary_search(&solver.ingest).is_err() {
                error!(
                    "solvers.{name}.ingest '{}' is not defined in ingestors",
                    solver.ingest
                );
                contains_error = true;
            }

            if !solver.exec.is_file() {
                error!(
                    "Failed to find solvers.{name}.exec. Either not a file or not found at {}",
                    solver.exec.to_string_lossy()
                );

                contains_error = true;
            } else {
                match check_executable(&solver.exec) {
                    Ok(is_executable) => {
                        if !is_executable {
                            error!(
                        "Solver {name} target {} is not executable, this might cause problems",
                        solver.exec.to_string_lossy()
                    );
                            contains_error = true;
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to determine in solvers.{name}.exec ({}) is an executable: {e}",
                            solver.exec.to_string_lossy()
                        );

                        contains_error = true;
                    }
                }
            }
        }

        for (test, value) in self.tests.iter_mut() {
            if value.solvers.is_empty() {
                warn!(
                    "Test {test} has an empty set of selected solvers. Falling back to all solvers"
                );
                value.solvers = self.solvers.keys().cloned().collect();
            } else {
                for solver in value.solvers.iter() {
                    if !self.solvers.contains_key(solver) {
                        error!("Test {test} references {solver} but {solver} is not defined");
                        contains_error = true;
                    }
                }
            }

            if value.path.is_none() && value.paths.is_empty() {
                error!("Test {test} contains neither 'path' nor 'paths' a test can't be a NOP");
                contains_error = true;
            } else if let Some(ref path) = value.path {
                if !value.paths.is_empty() {
                    warn!("Test {test} contains both 'path' and 'paths'. This will be treated as if 'path' is a member of 'paths'");
                } else {
                    // merge path into paths if neccessary
                    value.paths.push(path.clone());
                }
            }

            if value.timeout == 0 {
                error!("Test {test}.timeout cannot 0. This will lead to problems with evaluating some metrics.");
                contains_error = true;
            }
        }

        contains_error
    }
}

fn default_database_path() -> PathBuf {
    PathBuf::from_str("satan.db").unwrap()
}
