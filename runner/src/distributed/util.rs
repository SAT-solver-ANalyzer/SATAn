use crate::{config::ConfigErrors, database::ConnectionError};
use std::{
    ffi::{OsStr, OsString},
    io::{Error as IOError, ErrorKind},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
};
use tracing::{debug, error, warn};

pub fn prepend_hostname(input: &mut PathBuf) -> Result<(), ConfigErrors> {
    match nix::unistd::gethostname() {
        Ok(mut hostname) => {
            let file_name = input.file_name().unwrap_or(&OsStr::new("satan.db"));
            hostname.push(file_name);
            input.set_file_name(hostname);

            Ok(())
        }
        Err(error) => {
            error!(error = ?error, "Failed to retrieve hostname for node-local database: {error}");

            Err(ConfigErrors::DatabaseError(ConnectionError::ConfigError))
        }
    }
}

/// strips `prefix` from `source` and appends new prefix-less `source` to `base`
pub fn strip_prefix(source: OsString, prefix: OsString, mut base: OsString) -> OsString {
    // append old, original file name to the DONE_PREFIX
    // We have to jump around types a bit since indexing is messy with OsStr types
    debug_assert!(source.len() >= prefix.len());
    base.push(OsStr::from_bytes(&source.as_bytes()[prefix.len()..]));

    return base;
}

/// replace prefix with other prefix, factored out into a function to allow for better testing
#[inline]
pub fn reprefix(source: &OsString, old_prefix: &OsString, mut new_prefix: OsString) -> OsString {
    new_prefix.push(OsStr::from_bytes(
        &source.as_bytes()[old_prefix.as_bytes().len()..],
    ));

    new_prefix
}

/// rename a file with rename(2)
///
/// # Errors
///
/// This function will return an error if the original file was not found or another IO Error
/// happended, e.g., permission denied.
pub fn rename(source: &PathBuf, destination: &PathBuf) -> Result<(), IOError> {
    debug!(source = ?source, destination = ?destination, "Rename");
    let result = unsafe {
        // signature: rename(2), two *const char pointers
        nix::libc::rename(
            source.as_os_str().as_bytes().as_ptr() as *const i8,
            destination.as_os_str().as_bytes().as_ptr() as *const i8,
        )
    };

    if result == 0 {
        Ok(())
    } else if result == -1 {
        match nix::errno::errno() {
            nix::libc::ENOENT => Err(IOError::new(ErrorKind::NotFound, "")),
            nix::libc::EACCES => Err(IOError::new(ErrorKind::PermissionDenied, "")),
            errno => {
                warn!(errno = errno, "Failed to rename file");

                Err(IOError::new(
                    ErrorKind::Other,
                    "Encountered unexpected errno",
                ))
            }
        }
    } else {
        error!(result = result, "Unexpected result from rename");

        Err(IOError::new(
            ErrorKind::Other,
            format!("Unexpected result: {result}"),
        ))
    }
}
