use super::util::{rename, reprefix};
use once_cell::sync::Lazy;
use std::{
    ffi::OsString,
    ops::{Deref, DerefMut},
    path::PathBuf,
};
use tracing::{debug, error};

#[derive(Debug, Clone)]
/// A path that will rename the underlying object from PROCESSING_PREFIX to DONE_PREFIX when
/// dropped
pub struct WrappedPath {
    path: PathBuf,
}

impl Drop for WrappedPath {
    fn drop(&mut self) {
        // This is buggy, I think
        let file_name = self.path.file_name().unwrap().to_os_string();
        let done_file_name = reprefix(&file_name, &PROCESSING_PREFIX, DONE_PREFIX.clone());

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
