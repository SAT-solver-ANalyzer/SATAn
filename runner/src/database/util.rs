use cowstr::CowStr;
use std::collections::BTreeMap;

pub type IDMap = BTreeMap<CowStr, i32>;

// TODO: Factor out SQL schema and logic into this and adjacent modules
