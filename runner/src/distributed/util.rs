use crate::{config::ConfigErrors, database::ConnectionError};
use std::{ffi::OsStr, path::PathBuf};
use tracing::error;

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
