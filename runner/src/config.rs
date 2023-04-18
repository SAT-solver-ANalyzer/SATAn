use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::File, os::unix::prelude::MetadataExt, path::PathBuf};
use thiserror::Error;
use tracing::{error, warn};

use crate::executors::ExecutorError;

#[derive(Error, Debug)]
pub enum ConfigErrors {
    #[error("Globs were invalid")]
    InvalidGlobs(#[from] globset::Error),
    #[error("Executor not supported")]
    UnsupportedExecutor(String),
    #[error("Executor failed to load")]
    FailedLoadExecutor(#[from] ExecutorError),
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
}

impl SolverConfig {
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
        // TODO: Below is not performant nor clean, it should only work as a stand in

        // attempt to catch all errors instead of piece-by-piece to make debugging easier for users
        let mut contains_error = false;

        if self.solvers.is_empty() {
            error!("No solver was defined, unable to build a queue of tests");
            contains_error = true;
        }

        for (name, solver) in self.solvers.iter() {
            if !solver.exec.is_file() {
                error!(
                    "Failed to find {name}.exec. Either not a file or not found at {}",
                    solver.exec.to_string_lossy()
                );

                contains_error = true;
            } else {
                match File::open(&solver.exec).map(|file| file.metadata()) {
                    Ok(Ok(metadata)) => {
                        if (metadata.mode() & 0o111) == 0 {
                            warn!(
                        "Solver {name} target {:?} is not executable, this might cause problems",
                        solver.exec
                    );
                        }
                    }
                    Ok(Err(e)) | Err(e) => {
                        error!(
                            "Failed to lookup mode of solver {} for {}: {e}",
                            solver.exec.to_string_lossy(),
                            name
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
