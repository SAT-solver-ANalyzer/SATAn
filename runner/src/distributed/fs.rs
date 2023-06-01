use super::util::rename;
use once_cell::sync::Lazy;
use std::{
    ffi::{OsStr, OsString},
    ops::{Deref, DerefMut},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
};
use tracing::{debug, error};

#[derive(Debug, Clone)]
pub struct WrappedPath {
    path: PathBuf,
}

impl Drop for WrappedPath {
    fn drop(&mut self) {
        let file_name = self.path.file_name().unwrap().to_os_string();
        let mut done_file_name = DONE_PREFIX.clone();
        // append old, original file name to the DONE_PREFIX
        // We have to jump around types a bit since indexing is messy with OsStr types
        done_file_name.push(OsStr::from_bytes(
            &file_name.as_bytes()[PROCESSING_PREFIX.len()..],
        ));

        let mut done_file_path = self.path.clone();
        done_file_path.set_file_name(done_file_name);

        match rename(&self.path, &done_file_path) {
            Ok(()) => debug!(done_path = ?done_file_path, "Finished rename for file"),
            Err(error) => error!(error = ?error, "Failed to cleanup processing file"),
        }
    }
}

impl WrappedPath {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Deref for WrappedPath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl DerefMut for WrappedPath {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.path
    }
}

pub static PROCESSING_PREFIX: Lazy<OsString> = Lazy::new(|| {
    let mut string = OsString::new();
    string.push("[processing]_");
    string
});

pub static DONE_PREFIX: Lazy<OsString> = Lazy::new(|| {
    let mut string = OsString::new();
    string.push("[done]_");
    string
});
