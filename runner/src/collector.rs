#[cfg(feature = "distributed")]
use crate::distributed::{fs::PROCESSING_PREFIX, mpi::MPICollector};
use crate::{
    config::{CollectorConfig, ConfigErrors},
    distributed::fs::WrappedPath,
};
use cowstr::CowStr;
use globset::GlobBuilder;
use ignore::{DirEntry, WalkBuilder};
use itertools::Itertools;
use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    ops::{Deref, DerefMut},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
};
use tracing::{debug, error, info, warn};

/// map of testname -> Collector
pub type CollectorMap = BTreeMap<CowStr, Collector>;

#[derive(Debug)]
/// All possible collector variants
/// These should be initialized from `Collector::new`
/// (this is deliberately not made with dynamic dispatch to avoid the headache)
pub enum Collector {
    Glob {
        paths: Vec<PathBuf>,
    },
    GDB {
        paths: Vec<PathBuf>,
    },
    Grouped {
        collectors: Vec<Collector>,
    },
    #[cfg(feature = "distributed")]
    FS {
        inner: Box<Collector>,
    },
    #[cfg(feature = "distributed")]
    MPI(MPICollector),
}

/// A wrapper value for path like values
#[derive(Debug, Clone)]
pub enum PathValue {
    /// wrapped std PathBuf
    Buf(PathBuf),
    /// wrapped std::PathBuf with rename on drop
    Wrapped(WrappedPath),
}

impl Deref for PathValue {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Buf(pathbuf) => pathbuf,
            Self::Wrapped(wrapped) => wrapped,
        }
    }
}

impl DerefMut for PathValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Buf(pathbuf) => pathbuf,
            Self::Wrapped(ref mut wrapped) => wrapped,
        }
    }
}

/// primitve way to retrieve the tmp dir from the environment with defualt to /tmp
fn get_tmp_dir() -> PathBuf {
    env::var("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or(PathBuf::from("/tmp"))
}

impl<'a> Collector {
    pub fn load(config: &CollectorConfig) -> Result<Self, ConfigErrors> {
        // TODO: Inject FS and MPI collector here
        match config {
            CollectorConfig::GDB { server, tmp_dir } => {
                error!("The GDB isn't implemented yet, please use Glob instead");
                info!(
                    "Would save GDB tests to {:?} from {}",
                    tmp_dir.clone().unwrap_or_else(get_tmp_dir),
                    server
                );

                Ok(Self::GDB { paths: Vec::new() })
            }
            // NOTE: this is a stub because the grouped collectors can only be resolved once all
            // other collectors are built
            CollectorConfig::Grouped { collectors: _ } => Ok(Self::Grouped {
                collectors: Vec::new(),
            }),
            CollectorConfig::Glob { path, paths, glob } => {
                let glob = GlobBuilder::new(glob.as_str()).build()?;
                let (first, others) = paths.split_first().unwrap();
                let mut builder = WalkBuilder::new(first.as_str());

                debug!("Filtering with glob: {glob:?}");
                // add other paths
                others.iter().for_each(|path| {
                    builder.add(path.as_str());
                });
                if let Some(path) = path {
                    builder.add(path.as_str());
                }

                Ok(Self::Glob {
                    paths: builder
                        .build()
                        .filter_map(Result::ok)
                        .map(DirEntry::into_path)
                        .collect_vec(),
                })
            }
        }
    }

    /// create an empty grouped collector
    pub fn grouped() -> Self {
        Self::Grouped {
            collectors: Vec::new(),
        }
    }

    /// join multiple collectors into a single grouped collector
    /// this will if possible reuse existing grouped collectors
    pub fn join(self, other: Self) -> Self {
        match self {
            Self::Grouped { mut collectors } => {
                match other {
                    Self::Grouped {
                        collectors: other_collectors,
                    } => {
                        collectors.extend(other_collectors.into_iter());
                    }
                    non_grouped => {
                        collectors.push(non_grouped);
                    }
                };

                Self::Grouped { collectors }
            }
            non_grouped => match other {
                Self::Grouped { mut collectors } => {
                    collectors.push(non_grouped);

                    Self::Grouped { collectors }
                }
                other_non_grouped => Self::Grouped {
                    collectors: vec![non_grouped, other_non_grouped],
                },
            },
        }
    }
}

impl Iterator for Collector {
    type Item = PathValue;

    /// return accurate size for underlying iterator
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::GDB { paths } | Self::Glob { paths } => (paths.len(), Some(paths.len())),
            Self::Grouped { collectors } => {
                let len = collectors
                    .iter()
                    .fold(0, |acc, collector| acc + collector.size_hint().0);

                (len, Some(len))
            }
            Self::FS { inner } => inner.size_hint(),
            Self::MPI(_collector) => todo!(),
        }
    }

    /// return next test form initial test load
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::GDB { paths } | Self::Glob { paths } => paths.pop().map(PathValue::Buf),
            Self::Grouped { collectors } => {
                for collector in collectors {
                    let next = collector.next();

                    if next.is_some() {
                        return next;
                    }
                }

                None
            }
            Self::FS { inner } => {
                while let Some(path) = inner.next() {
                    match path {
                        PathValue::Wrapped { .. } => return Some(path),
                        PathValue::Buf(path) => {
                            if path.exists() {
                                // clone path and create a copy with the processing prefix
                                // this is quite convoluted to alow for not upcasting OsStrings
                                let mut new_path = path.clone();
                                let filename = new_path.file_name().unwrap_or(OsStr::new(""));
                                let mut joined_filename = OsString::with_capacity(
                                    filename.len() + PROCESSING_PREFIX.len(),
                                );
                                joined_filename.push(PROCESSING_PREFIX.as_os_str());
                                joined_filename.push(filename);
                                new_path.set_file_name(&joined_filename);

                                // we can take advantage of the fact that rename should (after
                                // linux guidelines) be an atomic operation and as such should
                                // survi
                                let result = unsafe {
                                    // signature: rename(2), two *const char pointers
                                    nix::libc::rename(
                                        path.as_os_str().as_bytes().as_ptr() as *const i8,
                                        new_path.as_os_str().as_bytes().as_ptr() as *const i8,
                                    )
                                };

                                if result == 0 {
                                    return Some(PathValue::Wrapped(WrappedPath::new(path)));
                                } else {
                                    match nix::errno::errno() {
                                        nix::libc::ENOENT => {
                                            debug!(
                                                path = ?path,
                                                "Skipped since it wasn't found between check and rename"
                                            )
                                        }
                                        nix::libc::EACCES => {
                                            warn!(path = ?path, "Failed to access due to permission error");
                                        }
                                        errno => {
                                            error!(
                                                path = ?path,
                                                errno = errno,
                                                "Failed to rename path for processing"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                None
            }
            Self::MPI(_collector) => todo!(),
        }
    }
}
