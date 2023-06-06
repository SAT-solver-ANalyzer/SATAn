use crate::distributed::{
    fs::{DONE_PREFIX, PROCESSING_PREFIX},
    util::strip_prefix,
};
use std::ffi::OsString;

use super::util::reprefix;

#[test]
pub fn reprefix_done() {
    let prefix = DONE_PREFIX.clone();
    let file_name = OsString::from("test.cnf");
    let mut joined = DONE_PREFIX.clone();
    joined.push(&file_name);

    let mut new_joined = PROCESSING_PREFIX.clone();
    new_joined.push(&file_name);

    assert_eq!(
        reprefix(&joined, &prefix, PROCESSING_PREFIX.clone()),
        new_joined
    );
    assert_eq!(file_name, strip_prefix(joined, prefix, OsString::new()));
}

#[test]
pub fn strip_prefix_done() {
    let file_name = OsString::from("[done]_aj;sdkjf.cnf");
    let prefix = DONE_PREFIX.clone();

    assert_eq!(
        OsString::from("aj;sdkjf.cnf"),
        strip_prefix(file_name, prefix, OsString::new())
    );
}

#[test]
pub fn strip_prefix_processing() {
    let file_name = OsString::from("[processing]_aj;sdkjf.cnf");
    let prefix = PROCESSING_PREFIX.clone();

    assert_eq!(
        OsString::from("aj;sdkjf.cnf"),
        strip_prefix(file_name, prefix, OsString::new())
    );
}
