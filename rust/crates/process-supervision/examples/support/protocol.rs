use serde::Deserialize;
use std::path::PathBuf;

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
pub struct Config {
    pub basedir: PathBuf,
    pub launcher: PathBuf,
    pub params_root: PathBuf,
    pub prefix: String,
    pub endpoint: String,
    pub processes: Vec<Process>,
}

#[derive(Deserialize)]
pub struct Process {
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub sigkill: bool,
    #[serde(default)]
    pub restart_if_crash: bool,
    #[serde(flatten)]
    pub launch: Launch,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Launch {
    Native {
        cwd: PathBuf,
        argv: Vec<String>,
    },
    Persistent {
        argv: Vec<String>,
        identity: String,
        param: String,
    },
}

#[derive(Deserialize)]
pub struct Race {
    pub name: String,
    pub release: PathBuf,
    pub pid: i32,
}

#[derive(Deserialize)]
pub struct Request {
    #[serde(default)]
    pub acknowledge: bool,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Action {
    Start {
        name: String,
    },
    Restart {
        name: String,
    },
    Prepare {
        name: String,
    },
    State {
        name: String,
    },
    Stop {
        name: String,
        #[serde(default = "yes")]
        retry: bool,
        #[serde(default = "yes")]
        block: bool,
        signal: Option<i32>,
    },
    Signal {
        name: String,
        signal: i32,
    },
    Ensure {
        allowed: Vec<String>,
        #[serde(default)]
        not_run: Vec<String>,
        race: Option<Race>,
    },
    Exit,
}
