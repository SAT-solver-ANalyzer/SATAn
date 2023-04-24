use crate::config::ConfigErrors;
use duckdb::{params, Connection};
use std::collections::BTreeMap;

pub type IDMap = BTreeMap<String, i32>;

pub fn retrieve_ids(connection: &Connection, query: &str) -> Result<IDMap, ConfigErrors> {
    connection
        .prepare_cached(query)?
        .query_map(params![], |row| Ok((row.get(0)?, row.get(1)?)))?
        .try_fold(BTreeMap::new(), |mut init, result| {
            let (id, name) = result?;
            init.insert(name, id);

            Ok::<IDMap, ConfigErrors>(init)
        })
}
