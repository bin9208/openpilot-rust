use crate::{Descriptor, RustAvailability, SourceProcess};
use std::{ffi::OsString, fs::File, io::Read, os::unix::fs::PermissionsExt, path::PathBuf};

pub struct Installation {
    pub root: PathBuf,
    pub binaries: PathBuf,
}

pub enum Requirement {
    Executable(PathBuf),
    Directory(PathBuf),
    File(PathBuf),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("installed artifact {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("installed native executable unavailable: {}", .0.display())]
    Executable(PathBuf),
    #[error("installed companion directory unavailable: {}", .0.display())]
    Directory(PathBuf),
    #[error("installed companion file must contain real nonempty bytes: {}", .0.display())]
    File(PathBuf),
}

pub struct Binding {
    pub argv: Vec<OsString>,
    pub cwd: PathBuf,
    pub requirements: Vec<Requirement>,
    pub pid_key: Option<&'static str>,
}

impl Binding {
    pub fn validate(&self) -> Result<(), Error> {
        for required in &self.requirements {
            match required {
                Requirement::File(path) => {
                    let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?;
                    if !metadata.is_file() || metadata.len() == 0 {
                        return Err(Error::File(path.clone()));
                    }
                    const POINTER: &[u8; 42] = b"version https://git-lfs.github.com/spec/v1";
                    if metadata.len() >= 42 {
                        let mut file = File::open(path).map_err(|source| Error::Io {
                            path: path.clone(),
                            source,
                        })?;
                        let mut prefix = [0; POINTER.len()];
                        file.read_exact(&mut prefix).map_err(|source| Error::Io {
                            path: path.clone(),
                            source,
                        })?;
                        if prefix == *POINTER {
                            return Err(Error::File(path.clone()));
                        }
                    }
                }
                Requirement::Directory(path) => {
                    if !path.is_dir() {
                        return Err(Error::Directory(path.clone()));
                    }
                }
                Requirement::Executable(path) => {
                    let mut file = File::open(path).map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?;
                    let metadata = file.metadata().map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?;
                    let mut magic = [0; 4];
                    file.read_exact(&mut magic).map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?;
                    if !metadata.is_file()
                        || metadata.permissions().mode() & 0o111 == 0
                        || magic != *b"\x7fELF"
                    {
                        return Err(Error::Executable(path.clone()));
                    }
                }
            }
        }
        Ok(())
    }

    fn directory(&mut self, argument: &str, directory: PathBuf) {
        self.argv
            .extend([argument.into(), directory.clone().into_os_string()]);
        self.requirements.push(Requirement::Directory(directory));
    }
}

pub fn resolve(entry: Descriptor, installation: &Installation) -> Option<Binding> {
    let binary = match entry.rust {
        RustAvailability::NotPorted => return None,
        RustAvailability::Candidate { binary, .. } => installation.binaries.join(binary),
    };
    let cwd = match entry.source {
        SourceProcess::Native { cwd, .. } => installation.root.join(cwd),
        SourceProcess::Python { .. } | SourceProcess::Persistent { .. } => {
            installation.root.clone()
        }
    };
    let mut bound = Binding {
        argv: vec![binary.clone().into_os_string()],
        cwd: cwd.clone(),
        requirements: vec![
            Requirement::Executable(binary),
            Requirement::Executable(installation.binaries.join("openpilot-process-child")),
            Requirement::Directory(cwd),
        ],
        pid_key: match entry.source {
            SourceProcess::Persistent { pid_key, .. } => Some(pid_key),
            SourceProcess::Native { .. } | SourceProcess::Python { .. } => None,
        },
    };
    if let SourceProcess::Native { argv, .. } = entry.source {
        bound.argv.extend(argv.iter().skip(1).map(OsString::from));
    }
    match entry.name {
        "carrot_cluster" => cluster(&mut bound, installation),
        "webjoystick" => {
            bound.directory("--root", installation.root.clone());
            let openssl = installation.binaries.join("openssl");
            bound
                .argv
                .extend(["--openssl".into(), openssl.clone().into_os_string()]);
            bound.requirements.push(Requirement::Executable(openssl));
        }
        "card" => {
            bound.directory("--root", installation.root.clone());
            bound.directory("--numerics", installation.binaries.join("card-numerics"));
        }
        "xiaoge_data" => {
            bound.directory("--root", installation.root.clone());
            for name in ["lane.onnx", "v_asm_model.onnx"] {
                bound.requirements.push(Requirement::File(
                    installation
                        .root
                        .join("openpilot/selfdrive/carrot/xiaoge/assets")
                        .join(name),
                ));
            }
        }
        "carrot_man" => bound.requirements.push(Requirement::Directory(
            installation.binaries.join("carrot-geos"),
        )),
        "carrot_server" => {
            bound
                .requirements
                .push(Requirement::Directory(installation.binaries.join("brotli")));
            for name in ["alert_camera.svg", "alert_police.svg"] {
                bound.requirements.push(Requirement::File(
                    installation
                        .root
                        .join("openpilot/selfdrive/carrot/web/assets")
                        .join(name),
                ));
            }
        }
        "ui" => bound.directory("--source-root", installation.root.clone()),
        "modeld" => bound.directory(
            "--trusted-catalog",
            installation.binaries.join("driving-models"),
        ),
        "dmonitoringmodeld" => bound.directory(
            "--trusted-catalog",
            installation.binaries.join("driver-models"),
        ),
        "controlsd" => {
            bound.directory(
                "--numerics",
                installation.binaries.join("controlsd-numerics"),
            );
            bound.directory(
                "--assets",
                installation.root.join("opendbc/car/torque_data"),
            );
        }
        "torqued" => bound.directory("--numerics", installation.binaries.join("torqued-numerics")),
        "radarcan" => {
            bound.directory(
                "--numerics",
                installation.binaries.join("radarcan-numerics"),
            );
            bound.directory("--dbc", installation.binaries.join("radarcan-dbc"));
        }
        "plannerd" => {
            let solver = installation.binaries.join("plannerd-acados");
            bound
                .argv
                .extend(["--solver".into(), solver.clone().into_os_string()]);
            bound.requirements.push(Requirement::Directory(solver));
        }
        "pandad" => {
            bound.directory("--basedir", installation.root.clone());
            let firmware = installation.root.join("panda/board/obj");
            bound.directory("--firmware", firmware.clone());
            for name in [
                "panda.bin.signed",
                "panda_h7.bin.signed",
                "panda_tesla_wake.bin.signed",
                "panda_h7_tesla_wake.bin.signed",
                "bootstub.panda.bin",
                "bootstub.panda_h7.bin",
            ] {
                bound
                    .requirements
                    .push(Requirement::File(firmware.join(name)));
            }
            bound.requirements.push(Requirement::Executable(
                installation.binaries.join("openpilot-pandad"),
            ));
        }
        "jetlinkd" => bound.argv.push("--run".into()),
        "soundd" => bound.directory(
            "--assets",
            installation.root.join("openpilot/selfdrive/assets"),
        ),
        _ => {}
    }
    Some(bound)
}

fn cluster(bound: &mut Binding, installation: &Installation) {
    bound.directory("--root", installation.root.clone());
    for name in ["openpilot-cluster", "ffmpeg", "ffprobe"] {
        bound
            .requirements
            .push(Requirement::Executable(installation.binaries.join(name)));
    }
    for name in [
        "rust/bin/cluster-replay/freesansbold.ttf",
        "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf",
        "openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png",
        "openpilot/selfdrive/assets/images/speed_bg.png",
        "openpilot/selfdrive/assets/images/traffic_red.png",
        "openpilot/selfdrive/assets/images/traffic_green.png",
        "openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png",
        "openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png",
        "openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png",
        "openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png",
    ] {
        bound
            .requirements
            .push(Requirement::File(installation.root.join(name)));
    }
}

/// Optional body teleoperation is available only with its real native providers.
pub fn bodyteleop_available(installation: &Installation) -> bool {
    crate::catalog(crate::ImportConfig::from_environment(true))
        .into_iter()
        .find(|entry| entry.name == "webjoystick")
        .and_then(|entry| resolve(entry, installation))
        .is_some_and(|bound| bound.validate().is_ok())
}
