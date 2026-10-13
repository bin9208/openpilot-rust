use super::{
    transport::{agent, STREAM_BUFFER},
    WebDav,
};
use crate::{Error, Observer, Progress, Value};
use std::{
    io::{self, Read},
    path::Path,
    time::Duration,
};
mod failures;
mod fixture;
mod policy;
mod storage;
