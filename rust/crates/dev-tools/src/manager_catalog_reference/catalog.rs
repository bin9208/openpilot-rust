// Source-derived constructor order from process_config.py; no candidate catalog import.
use super::{ops as o, Context};
use crate::seal::Result;
use openpilot_log_format::JsonValue;
#[derive(Clone, Copy)]
enum Enabled {
    Yes,
    No,
    Webcam,
    NotWebcam,
    NotDarwin,
    NotPc,
    WebcamOrNotPc,
    Tici,
    Body,
    NotExternal,
}
pub struct Config {
    pub pc: JsonValue,
    pub tici: JsonValue,
    pub webcam: bool,
    pub external: bool,
    pub darwin: bool,
    pub body: bool,
}
pub struct Entry {
    pub name: &'static str,
    kind: &'static str,
    module: Option<&'static str>,
    cwd: Option<&'static str>,
    argv: &'static [&'static str],
    pid_key: Option<&'static str>,
    enabled: Enabled,
    restart: bool,
    pub callback: &'static str,
}
impl Entry {
    fn python(
        name: &'static str,
        module: &'static str,
        callback: &'static str,
        enabled: Enabled,
        restart: bool,
    ) -> Self {
        Self {
            name,
            kind: "PythonProcess",
            module: Some(module),
            cwd: None,
            argv: &[],
            pid_key: None,
            enabled,
            restart,
            callback,
        }
    }
    fn native(
        name: &'static str,
        cwd: &'static str,
        argv: &'static [&'static str],
        callback: &'static str,
        enabled: Enabled,
    ) -> Self {
        Self {
            name,
            kind: "NativeProcess",
            module: None,
            cwd: Some(cwd),
            argv,
            pid_key: None,
            enabled,
            restart: false,
            callback,
        }
    }
    fn daemon(name: &'static str, module: &'static str, pid_key: &'static str) -> Self {
        Self {
            name,
            kind: "DaemonProcess",
            module: Some(module),
            cwd: None,
            argv: &[],
            pid_key: Some(pid_key),
            enabled: Enabled::Yes,
            restart: false,
            callback: "always_run",
        }
    }
    pub fn snapshot(&self, config: &Config) -> Result<JsonValue> {
        let enabled = match self.enabled {
            Enabled::Yes => o::boolean(true),
            Enabled::No => o::boolean(false),
            Enabled::Webcam => o::boolean(config.webcam),
            Enabled::NotWebcam => o::boolean(!config.webcam),
            Enabled::NotDarwin => o::boolean(!config.darwin),
            Enabled::NotPc => o::boolean(!o::truth(&config.pc)),
            Enabled::WebcamOrNotPc => o::boolean(config.webcam || !o::truth(&config.pc)),
            Enabled::Tici => config.tici.clone(),
            Enabled::Body => o::boolean(config.body),
            Enabled::NotExternal => o::boolean(!config.external),
        };
        o::object(&[
            ("name", o::text(self.name)),
            ("kind", o::text(self.kind)),
            ("module", o::optional_text(self.module)),
            ("cwd", o::optional_text(self.cwd)),
            (
                "argv",
                if self.kind == "NativeProcess" {
                    o::array(&self.argv.iter().map(|v| o::text(v)).collect::<Vec<_>>())?
                } else {
                    o::null()
                },
            ),
            ("pid_key", o::optional_text(self.pid_key)),
            ("enabled", enabled),
            ("sigkill", o::boolean(false)),
            ("restart_if_crash", o::boolean(self.restart)),
            ("daemon", o::boolean(false)),
        ])
    }
}
pub fn entries() -> Vec<Entry> {
    vec![
        Entry::daemon(
            "manage_athenad",
            "openpilot.system.athena.manage_athenad",
            "AthenadPid",
        ),
        Entry::native(
            "loggerd",
            "openpilot/system/loggerd",
            &["./loggerd"],
            "logging",
            Enabled::Yes,
        ),
        Entry::native(
            "encoderd",
            "openpilot/system/loggerd",
            &["./encoderd"],
            "only_onroad",
            Enabled::Yes,
        ),
        Entry::native(
            "stream_encoderd",
            "openpilot/system/loggerd",
            &["./encoderd", "--stream"],
            "notcar",
            Enabled::Yes,
        ),
        Entry::native(
            "carrot_vision_encoderd",
            "openpilot/system/loggerd",
            &["./encoderd", "--carrot-vision-road"],
            "all:iscar,enable_webrtc",
            Enabled::Yes,
        ),
        Entry::native(
            "youtube_low_encoderd",
            "openpilot/system/loggerd",
            &["./encoderd", "--youtube-low"],
            "all:only_onroad,enable_youtube_low_encoder",
            Enabled::Yes,
        ),
        Entry::native(
            "youtube_medium_encoderd",
            "openpilot/system/loggerd",
            &["./encoderd", "--youtube-medium"],
            "all:only_onroad,enable_youtube_medium_encoder",
            Enabled::Yes,
        ),
        Entry::native(
            "youtube_encoderd",
            "openpilot/system/loggerd",
            &["./encoderd", "--youtube"],
            "all:only_onroad,enable_youtube_encoder",
            Enabled::Yes,
        ),
        Entry::native(
            "youtube_wide_encoderd",
            "openpilot/system/loggerd",
            &["./encoderd", "--youtube-wide"],
            "all:only_onroad,enable_youtube_wide_encoder",
            Enabled::Yes,
        ),
        Entry::python(
            "logmessaged",
            "openpilot.system.logmessaged",
            "always_run",
            Enabled::Yes,
            false,
        ),
        Entry::native(
            "camerad",
            "openpilot/system/camerad",
            &["./camerad"],
            "driverview",
            Enabled::NotWebcam,
        ),
        Entry::python(
            "webcamerad",
            "openpilot.tools.webcam.camerad",
            "driverview",
            Enabled::Webcam,
            false,
        ),
        Entry::python(
            "proclogd",
            "openpilot.system.proclogd",
            "only_onroad",
            Enabled::NotDarwin,
            false,
        ),
        Entry::python(
            "journald",
            "openpilot.system.journald",
            "only_onroad",
            Enabled::NotDarwin,
            false,
        ),
        Entry::python(
            "micd",
            "openpilot.system.micd",
            "iscar",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "timed",
            "openpilot.system.timed",
            "always_run",
            Enabled::NotPc,
            false,
        ),
        Entry::python(
            "modeld",
            "openpilot.selfdrive.modeld.modeld",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "dmonitoringmodeld",
            "openpilot.selfdrive.modeld.dmonitoringmodeld",
            "enable_dm",
            Enabled::WebcamOrNotPc,
            false,
        ),
        Entry::python(
            "sensord",
            "openpilot.system.sensord.sensord",
            "only_onroad",
            Enabled::NotPc,
            false,
        ),
        Entry::python(
            "ui",
            "openpilot.selfdrive.ui.ui",
            "always_run",
            Enabled::Yes,
            true,
        ),
        Entry::python(
            "soundd",
            "openpilot.selfdrive.ui.soundd",
            "driverview",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "locationd",
            "openpilot.selfdrive.locationd.locationd",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::native(
            "_pandad",
            "openpilot/selfdrive/pandad",
            &["./pandad"],
            "always_run",
            Enabled::No,
        ),
        Entry::python(
            "calibrationd",
            "openpilot.selfdrive.locationd.calibrationd",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "torqued",
            "openpilot.selfdrive.locationd.torqued",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "controlsd",
            "openpilot.selfdrive.controls.controlsd",
            "all:not_joystick,iscar",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "joystickd",
            "openpilot.tools.joystick.joystickd",
            "any:joystick,notcar",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "selfdrived",
            "openpilot.selfdrive.selfdrived.selfdrived",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "card",
            "openpilot.selfdrive.car.card",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "deleter",
            "openpilot.system.loggerd.deleter",
            "always_run",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "dmonitoringd",
            "openpilot.selfdrive.monitoring.dmonitoringd",
            "enable_dm",
            Enabled::WebcamOrNotPc,
            false,
        ),
        Entry::python(
            "qcomgpsd",
            "openpilot.system.qcomgpsd.qcomgpsd",
            "qcomgps",
            Enabled::Tici,
            false,
        ),
        Entry::python(
            "navd",
            "openpilot.selfdrive.navd.navd",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "pandad",
            "openpilot.selfdrive.pandad.pandad",
            "always_run",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "paramsd",
            "openpilot.selfdrive.locationd.paramsd",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "lagd",
            "openpilot.selfdrive.locationd.lagd",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "ubloxd",
            "openpilot.system.ubloxd.ubloxd",
            "ublox",
            Enabled::Tici,
            false,
        ),
        Entry::python(
            "pigeond",
            "openpilot.system.ubloxd.pigeond",
            "ublox",
            Enabled::Tici,
            false,
        ),
        Entry::python(
            "plannerd",
            "openpilot.selfdrive.controls.plannerd",
            "not_long_maneuver",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "maneuversd",
            "openpilot.tools.longitudinal_maneuvers.maneuversd",
            "long_maneuver",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "lateral_maneuversd",
            "openpilot.tools.lateral_maneuvers.lateral_maneuversd",
            "lat_maneuver",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "radard",
            "openpilot.selfdrive.carrot.radar.radard_dpath",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "radarcan",
            "openpilot.selfdrive.carrot.radar.radarcan",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "hardwared",
            "openpilot.system.hardware.hardwared",
            "always_run",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "jetlinkd",
            "openpilot.selfdrive.modeld.jetlink.daemon",
            "always_run",
            Enabled::Tici,
            false,
        ),
        Entry::python(
            "modem",
            "openpilot.system.hardware.tici.modem",
            "always_run",
            Enabled::Tici,
            false,
        ),
        Entry::python(
            "tombstoned",
            "openpilot.system.tombstoned",
            "always_run",
            Enabled::NotPc,
            false,
        ),
        Entry::python(
            "updated",
            "openpilot.system.updated.updated",
            "enable_updated",
            Enabled::NotPc,
            false,
        ),
        Entry::python(
            "statsd",
            "openpilot.system.statsd",
            "always_run",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "feedbackd",
            "openpilot.selfdrive.ui.feedback.feedbackd",
            "only_onroad",
            Enabled::Yes,
            false,
        ),
        Entry::native(
            "bridge",
            "openpilot/cereal/messaging",
            &["./bridge"],
            "notcar",
            Enabled::Yes,
        ),
        Entry::python(
            "webrtcd",
            "openpilot.system.webrtc.webrtcd",
            "notcar",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "carrot_webrtcd",
            "openpilot.system.webrtc.carrot_webrtcd",
            "all:iscar,enable_webrtc",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "webjoystick",
            "openpilot.tools.bodyteleop.web",
            "notcar",
            Enabled::Body,
            false,
        ),
        Entry::python(
            "joystick",
            "openpilot.tools.joystick.joystick_control",
            "all:joystick,iscar",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "carrot_man",
            "openpilot.selfdrive.carrot.carrot_man",
            "always_run",
            Enabled::Yes,
            true,
        ),
        Entry::python(
            "carrot_navi",
            "openpilot.selfdrive.carrot.carrot_navi",
            "always_run",
            Enabled::Yes,
            true,
        ),
        Entry::python(
            "carrot_server",
            "openpilot.selfdrive.carrot.carrot_server",
            "always_run",
            Enabled::NotExternal,
            false,
        ),
        Entry::python(
            "carrot_bluetooth",
            "openpilot.selfdrive.carrot.bluetooth.daemon",
            "always_run",
            Enabled::Tici,
            true,
        ),
        Entry::python(
            "cweb_push",
            "openpilot.selfdrive.carrot.cweb_push",
            "always_run",
            Enabled::NotPc,
            false,
        ),
        Entry::python(
            "carrot_cluster",
            "openpilot.selfdrive.carrot.cluster_autorun",
            "enable_cluster_hud",
            Enabled::Yes,
            true,
        ),
        Entry::python(
            "xiaoge_data",
            "openpilot.selfdrive.carrot.xiaoge_data",
            "enable_xiaoge_data",
            Enabled::Yes,
            false,
        ),
        Entry::python(
            "beep",
            "openpilot.selfdrive.controls.beep",
            "c3x_lite",
            Enabled::Tici,
            false,
        ),
    ]
}
pub fn config(request: &JsonValue, context: &Context) -> Result<Config> {
    let given = o::optional(request, "config")?;
    let explicit = o::truth(&given);
    let raw = if explicit {
        given
    } else {
        o::object(&[
            ("pc", o::boolean(!context.tici)),
            ("tici", o::boolean(context.tici)),
            ("webcam", o::boolean(context.webcam.is_some())),
            (
                "carrot_web_external",
                o::boolean(
                    context.carrot_web_external.as_deref() == Some(std::ffi::OsStr::new("1")),
                ),
            ),
            ("darwin", o::boolean(context.darwin)),
            (
                "bodyteleop_available",
                o::default(request, "bodyteleop_available", o::boolean(false))?,
            ),
        ])?
    };
    let pc = o::get(&raw, "pc")?;
    let tici = o::get(&raw, "tici")?;
    let (webcam, external) = if request.get("config").is_some() {
        (
            o::truth(&o::get(&raw, "webcam")?),
            o::truth(&o::get(&raw, "carrot_web_external")?),
        )
    } else {
        (
            context.webcam.is_some(),
            context.carrot_web_external.as_deref() == Some(std::ffi::OsStr::new("1")),
        )
    };
    let darwin = o::truth(&o::get(&raw, "darwin")?);
    let body = if o::truth(&o::optional(request, "body_discovery_missing_parent")?) {
        false
    } else {
        o::truth(&o::get(&raw, "bodyteleop_available")?)
    };
    Ok(Config {
        pc,
        tici,
        webcam,
        external,
        darwin,
        body,
    })
}
