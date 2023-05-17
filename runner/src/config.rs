use crate::{
    collector::TestCollector,
    database::ConnectionError,
    executors::ExecutorError,
    ingest::{IngestorMap, Ingestors},
};
use cowstr::CowStr;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::File, io::Error, os::unix::fs::MetadataExt, path::PathBuf};
use thiserror::Error;
use tracing::{debug, error, warn};

// check if a file is executable
pub fn check_executable(path: &PathBuf) -> bool {
    if !path.is_file() {
        debug!("{path:?} was not a file");
        true
    } else {
        match File::open(path).map(|file| file.metadata()) {
            Ok(Ok(metadata)) => (metadata.mode() & 0o111) == 0,
            Ok(Err(e)) | Err(e) => {
                debug!("{path:?} couldn't read metadata: {e}");
                true
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum ConfigErrors {
    #[error("Globs were invalid")]
    InvalidGlobs(#[from] globset::Error),
    #[error("Executor failed to load")]
    FailedLoadExecutor(#[from] ExecutorError),
    #[error("Ingestor failed to load")]
    FailedLoadIngestor,
    #[error("Metadata not found")]
    MetadataNotFound(#[from] Error),
    #[error("Database Connection failed")]
    DatabaseError(#[from] ConnectionError),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SolverConfig {
    // executor config, has yet to be fully structured
    pub executor: ExecutorConfig,
    // Solvers as generic executables with fixed parameters, this might be extended later on
    pub solvers: BTreeMap<CowStr, Solver>,
    // Tests as sets of test files, again only a stub for e.g., an interface of some kind
    pub tests: BTreeMap<CowStr, TestSet>,
    // Config for all ingestor related setups
    pub ingest: BTreeMap<CowStr, IngestorConfig>,

    #[serde(alias = "db")]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub delayed: bool,

    #[serde(alias = "logs", default)]
    pub tracing: TracingConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub enum TracingConfig {
    OpenTelemetry,
    Stdio,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self::Stdio
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    #[serde(default)]
    pub delayed: bool,
    pub batched: Option<BatchConfig>,

    #[cfg_attr(any(feature = "duckdb", feature = "duckdb"), serde(default))]
    pub connection: ConnectionConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BatchConfig {
    #[serde(default = "default_batch_number")]
    pub size: u32,
    pub timeout: Option<u32>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub enum ConnectionConfig {
    #[cfg(feature = "duckdb")]
    DuckDB {
        #[serde(default)]
        path: PathBuf,
    },

    #[cfg(feature = "rusqlite")]
    SQLite {
        #[serde(default)]
        path: PathBuf,
    },

    #[cfg(feature = "clickhouse")]
    ClickHouse {
        #[serde(with = "http_serde::uri")]
        server: http::Uri,
        database: String,
        user: Option<String>,
        password: Option<String>,
        connections: Option<u32>,
        lz4: Option<bool>,
        lz4hc: Option<u8>,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub enum IngestorConfig {
    Exec(ExecIngestConfig),
    Null,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExecIngestConfig {
    pub executable: PathBuf,
    #[serde(default)]
    pub params: CowStr,
    pub timeout: u64,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub enum ExecutorConfig {
    Local {
        #[serde(default = "affinity::get_core_num")]
        threads: usize,
        pinned: bool,
    },

    #[cfg(feature = "distributed")]
    Distributed {
        // type of work coordination
        synchronization: crate::sync::SynchronizationTypes,
        // wether to use the local executor internally to perform distributed tasks
        local: bool,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub enum CollectorConfig {
    Glob {
        #[serde(default)]
        paths: Vec<CowStr>,
        glob: CowStr,
        #[serde(default)]
        path: Option<CowStr>,
    },
    GDB {},
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TestSet {
    pub timeout: u32,
    pub collector: CollectorConfig,
    #[serde(default = "default_iter_number")]
    pub iterations: usize,
    #[serde(default)]
    pub solvers: Vec<CowStr>,
    #[serde(default)]
    pub params: Vec<CowStr>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Solver {
    pub exec: PathBuf,
    #[serde(default)]
    pub params: Vec<CowStr>,
    pub ingest: CowStr,
}

impl Solver {
    pub fn get_params(&self) -> String {
        self.params.iter().join(" ")
    }
}

impl TestSet {
    pub fn get_params(&self) -> String {
        self.params.iter().join(" ")
    }
}

impl SolverConfig {
    /// load, if possible, all ingestors
    pub fn load_ingestors(&self) -> Result<IngestorMap, ConfigErrors> {
        let mut ingestors = IngestorMap::new();
        let mut contains_error = false;

        for (name, config) in self.ingest.iter() {
            match Ingestors::load(config) {
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

    pub fn collectors(&mut self) -> Result<Vec<TestCollector>, (CowStr, ConfigErrors)> {
        self.tests.iter().try_fold(
            Vec::new(),
            |mut acc, (name, test)| match TestCollector::new(&test.collector) {
                Ok(collector) => {
                    acc.push(collector);

                    Ok(acc)
                }
                Err(error) => Err((name.clone(), error)),
            },
        )
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
            if let IngestorConfig::Exec(exec_config) = config {
                if check_executable(&exec_config.executable) {
                    error!("ingestor.{name}.executable must be a path to an executable file");
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
            } else if check_executable(&solver.exec) {
                error!(
                    "Solver {name} target {} is not executable, this might cause problems",
                    solver.exec.to_string_lossy()
                );
                contains_error = true;
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

            match &mut value.collector {
                CollectorConfig::GDB {} => todo!(),
                CollectorConfig::Glob {
                    glob: _,
                    path,
                    paths,
                } => {
                    if path.is_none() && paths.is_empty() {
                        error!(
                            "Test {test} contains neither 'path' nor 'paths' a test can't be a NOP"
                        );
                        contains_error = true;
                    } else if let Some(ref path) = path {
                        if !paths.is_empty() {
                            warn!("Test {test} contains both 'path' and 'paths'. This will be treated as if 'path' is a member of 'paths'");
                        } else {
                            // merge path into paths if neccessary
                            paths.push(path.clone());
                        }
                    }
                }
            }

            if value.timeout == 0 {
                error!("Test {test}.timeout cannot 0. This will lead to problems with evaluating some metrics.");
                contains_error = true;
            }
        }

        if self.database.delayed && self.database.batched.is_some() {
            warn!("Enabling both database.delayed and database.batched is not recommended");
        }

        match &self.database.connection {
            #[cfg(feature = "rusqlite")]
            ConnectionConfig::SQLite { path } => {
                if !path.is_file() && path.exists() {
                    error!(
                        "database.path for SQLite needs to be either regular file or an empty path"
                    );

                    contains_error = true;
                }
            }

            #[cfg(feature = "duckdb")]
            ConnectionConfig::DuckDB { path } => {
                if !path.is_file() && path.exists() {
                    error!(
                        "database.path for DuckDB needs to be either regular file or an empty path"
                    );

                    contains_error = true;
                }
            }

            #[cfg(feature = "clickhouse")]
            ConnectionConfig::ClickHouse {
                server: _,
                database: _,
                user,
                password,
                connections: _,
                lz4,
                lz4hc,
            } => {
                if (user.is_some() && password.is_none()) || (user.is_none() && password.is_some())
                {
                    error!("database.username: Either neither or both user and password need to be specified")
                }

                #[cfg(feature = "clickhouse-lz4")]
                if lz4.is_some() || lz4hc.is_some() {
                    warn!("This binary was compiled without clickhouse compression support, the settings will be ignored");
                }
            }
        }

        // TDOO: Add preflight checks for databases
        // - duckdb: path either empty or exists and file
        // - clickhouse:
        // -    Compression methods (what is enabled and compiled in)
        // -    passwword + username or neither

        contains_error
    }
}

fn default_batch_number() -> u32 {
    100
}

fn default_iter_number() -> usize {
    1
}

#[cfg(any(feature = "rusqlite", feature = "duckdb"))]
impl Default for ConnectionConfig {
    #[cfg(feature = "rusqlite")]
    fn default() -> Self {
        Self::SQLite {
            path: "./satan.db".into(),
        }
    }

    #[cfg(all(not(feature = "rusqlite"), feature = "duckdb"))]
    fn default() -> Self {
        Self::DuckDB {
            path: "./satan.db".into(),
        }
    }
}
