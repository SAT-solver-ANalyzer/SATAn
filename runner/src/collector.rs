use crate::{
    config::{CollectorConfig, ConfigErrors},
    executors::ExecutorError,
};
use cowstr::CowStr;
use globset::{GlobBuilder, GlobMatcher};
use ignore::WalkBuilder;
use itertools::Itertools;
use std::{borrow::Cow, path::Path};
use tracing::{debug, warn};

#[derive(Debug)]
pub enum TestCollector {
    Glob {
        paths: Vec<CowStr>,
        glob: GlobMatcher,
    },
    GDB {
        server: http::Uri,
    },
}

impl TestCollector {
    pub fn new(config: &CollectorConfig) -> Result<Self, ConfigErrors> {
        match config {
            CollectorConfig::GDB {} => todo!(),
            CollectorConfig::Glob {
                path: _,
                paths,
                glob,
            } => {
                let glob = GlobBuilder::new(glob.as_str()).build()?;

                Ok(Self::Glob {
                    paths: paths.clone(),
                    glob: glob.compile_matcher(),
                })
            }
        }
    }

    /// prepare test files, this only applies to collectors that have to retrieve test files
    /// explicitly
    pub fn prepare(&mut self) -> Result<(), ExecutorError> {
        match self {
            Self::Glob { .. } => Ok(()),
            Self::GDB { .. } => todo!(),
        }
    }

    pub fn iter(self) -> Result<Vec<Cow<'static, Path>>, ExecutorError> {
        match self {
            Self::Glob { paths, glob } => {
                let (first, others) = paths.split_first().unwrap();
                let mut builder = WalkBuilder::new(first.as_str());

                debug!("Filtering with glob: {glob:?}");
                // add other paths
                others.iter().for_each(|path| {
                    builder.add(path.as_str());
                });

                let paths = builder
                    .build()
                    .filter_map(|path| match path {
                        // TODO: Add a warning in the docs for this
                        Ok(path) => Some(path),
                        Err(e) => {
                            warn!(error = ?e, "Failed to search for tests for test: {e}");

                            None
                        }
                    })
                    .filter(|entry| glob.is_match(entry.path()))
                    .map(|path| Cow::from(path.into_path()))
                    .collect_vec();

                debug!("Found paths: {paths:?}");

                Ok(paths)
            }
            Self::GDB { .. } => todo!(),
        }
    }
}
