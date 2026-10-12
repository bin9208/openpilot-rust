mod protocol;
mod worker;
use super::{Error, Frame, Parser};
use crate::runtime_args::CornerSource;
use serde::{Deserialize, Serialize};
use std::{
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};
pub use worker::{Config, Worker};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParseRequest {
    pub generation: u64,
    pub index: usize,
    pub path: PathBuf,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub generation: u64,
    pub index: usize,
    pub path: PathBuf,
    pub result: Result<Vec<Frame>, String>,
}
#[derive(Serialize, Deserialize)]
enum Request {
    Configure { front_radar_only: bool },
    Parse(ParseRequest),
    Stop,
}

pub struct Session {
    parser: Parser,
    first: Option<f64>,
}
impl Session {
    #[must_use]
    pub fn new(source: CornerSource) -> Self {
        Self {
            parser: Parser::new(source),
            first: None,
        }
    }
    pub fn parse(&mut self, request: ParseRequest) -> Response {
        let result = self
            .parser
            .parse_file(&request.path)
            .map(|mut frames| {
                if let Some(minimum) = frames.iter().map(|frame| frame.t).reduce(f64::min) {
                    let first = *self.first.get_or_insert(minimum);
                    for frame in &mut frames {
                        frame.t -= first;
                    }
                    frames.sort_by(|left, right| left.t.total_cmp(&right.t));
                }
                frames
            })
            .map_err(|error| error.to_string());
        Response {
            generation: request.generation,
            index: request.index,
            path: request.path,
            result,
        }
    }
}

/// # Errors
/// Returns private IPC or malformed request errors; file errors are returned as responses.
pub fn run_helper(socket: &Path, source: CornerSource) -> Result<(), Error> {
    if let Ok(priority) = rustix::process::getpriority_process(None) {
        let _ = rustix::process::setpriority_process(None, (priority + 10).min(19));
    }
    let mut stream = UnixStream::connect(socket)?;
    let mut session = Session::new(source);
    loop {
        match protocol::read::<Request>(&mut stream)? {
            Request::Configure { front_radar_only } => {
                session.parser.set_front_radar_only(front_radar_only);
            }
            Request::Stop => return Ok(()),
            Request::Parse(request) => protocol::write(&mut stream, &session.parse(request))?,
        }
    }
}
