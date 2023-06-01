use crate::{config::ConfigErrors, database::ConnectionError};
use std::{
    ffi::OsStr,
    io::{Error as IOError, ErrorKind},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
};
use tracing::{error, warn};

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

pub fn rename(source: &PathBuf, destination: &PathBuf) -> Result<(), IOError> {
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
