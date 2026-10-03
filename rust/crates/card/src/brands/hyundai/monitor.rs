use super::{
    config::CarConfig,
    flags as f,
    parameters::setting_int,
    parser_inputs::{Channel, Inputs},
    Error,
};
use openpilot_params::Params;

pub struct Monitor {
    pub count: u32,
    hint_enabled: bool,
    hint: bool,
}

impl Monitor {
    pub fn new(config: &CarConfig, settings: &Params) -> Result<Self, Error> {
        let enabled =
            setting_int(settings, "HyundaiCameraSCC")? == 0 && config.flags & f::CAMERA_SCC == 0;
        settings.put_bool("HyundaiCameraSccHint", false)?;
        Ok(Self {
            count: 0,
            hint_enabled: enabled,
            hint: false,
        })
    }

    pub fn update(
        &mut self,
        inputs: &mut Inputs,
        config: &CarConfig,
        settings: &Params,
        now: u64,
    ) -> Result<(), Error> {
        let fd = config.flags & f::CANFD != 0;
        let hint_name = if fd { "SCC_CONTROL" } else { "SCC12" };
        if self.hint_enabled
            && !self.hint
            && inputs
                .cam
                .dbc
                .names
                .get(hint_name)
                .is_some_and(|address| inputs.cam.seen_addresses.contains(address))
        {
            self.hint = true;
            settings.put_bool("HyundaiCameraSccHint", true)?;
        }
        if self.count > 200 {
            return Ok(());
        }
        if settings.get_bool("ControlsReady")? {
            self.count += 1;
        }
        inputs.diagnostics.monitor(self.count, inputs.alt.is_some());
        if self.count == 50 {
            inputs.pt.controls_ready = true;
            inputs.cam.controls_ready = true;
            if let Some(alt) = &mut inputs.alt {
                alt.controls_ready = true;
            }
        }
        if !fd {
            match self.count {
                104 => {
                    if !inputs.capture((Channel::Cam, "FCA11", "fca11"), false, now)? {
                        inputs.capture((Channel::Pt, "FCA11", "fca11"), false, now)?;
                    }
                    inputs.capture((Channel::Cam, "LKAS11", "lkas11"), false, now)?;
                    inputs.capture((Channel::Pt, "CLU11", "clu11"), false, now)?;
                }
                105 => {
                    if !config.longitudinal || config.flags & f::CAMERA_SCC != 0 {
                        let channel = if config.flags & f::CAMERA_SCC != 0 {
                            Channel::Cam
                        } else {
                            Channel::Pt
                        };
                        for (name, key) in [
                            ("SCC11", "scc11"),
                            ("SCC12", "scc12"),
                            ("SCC13", "scc13"),
                            ("SCC14", "scc14"),
                        ] {
                            inputs.capture((channel, name, key), false, now)?;
                        }
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        match self.count {
            120 => {
                let channel = if config.flags & f::CAMERA_SCC != 0 {
                    Channel::Cam
                } else {
                    Channel::Pt
                };
                inputs.capture((channel, "SCC_CONTROL", "scc_control"), false, now)?;
            }
            121 => {
                for (channel, name, key) in [
                    (Channel::Pt, "TCS", "tcs"),
                    (Channel::Pt, "MDPS", "mdps"),
                    (Channel::Cam, "LFA", "lfa"),
                    (Channel::Cam, "LFA_ALT", "lfa_alt"),
                    (Channel::Cam, "LFAHDA_CLUSTER", "lfahda_cluster"),
                ] {
                    inputs.capture((channel, name, key), false, now)?;
                }
            }
            122 => {
                for (name, key) in [
                    ("ADRV_0x161", "adrv_0x161"),
                    ("ADRV_0x200", "adrv_0x200"),
                    ("ADRV_0x1ea", "adrv_0x1ea"),
                    ("ADRV_0x160", "adrv_0x160"),
                    ("CCNC_0x162", "ccnc_0x162"),
                ] {
                    inputs.capture((Channel::Cam, name, key), false, now)?;
                }
            }
            123 => {
                if config.candidate == "KIA_PV5" {
                    inputs.capture(
                        (Channel::Pt, "CANFD_HDA_INFO_364", "hda_info_4a3"),
                        false,
                        now,
                    )?;
                    inputs.capture(
                        (Channel::Pt, "CANFD_NAVI_PROFILE_093", "navi_profile_4be"),
                        false,
                        now,
                    )?;
                    inputs.capture(
                        (Channel::Alt, "CANFD_NAVI_STATUS_380", "navi_status_380"),
                        false,
                        now,
                    )?;
                } else {
                    for (name, key) in [
                        ("HDA_INFO_4A3", "hda_info_4a3"),
                        ("NEW_MSG_4B4", "navi_position_4b4"),
                        ("NEW_MSG_4B9", "navi_segment_4b9"),
                        ("NEW_MSG_4BE", "navi_profile_4be"),
                    ] {
                        inputs.capture((Channel::Pt, name, key), false, now)?;
                    }
                }
                inputs.capture(
                    (Channel::Pt, "STEER_TOUCH_2AF", "steer_touch_2af"),
                    false,
                    now,
                )?;
            }
            124 => {
                inputs.capture(
                    (Channel::Pt, config.button_message(), "cruise_buttons_msg"),
                    false,
                    now,
                )?;
                if !inputs.capture((Channel::Cam, "CAM_0x362", "cam_0x362"), false, now)? {
                    inputs.capture((Channel::Alt, "CAM_0x362", "cam_0x362"), false, now)?;
                }
                if !inputs.capture((Channel::Alt, "CAM_0x2a4", "cam_0x2a4"), false, now)? {
                    inputs.capture((Channel::Cam, "CAM_0x2a4", "cam_0x2a4"), false, now)?;
                }
            }
            125 => {
                inputs.capture(
                    (
                        Channel::Pt,
                        "MANUAL_SPEED_LIMIT_ASSIST",
                        "manual_speed_limit_assist",
                    ),
                    true,
                    now,
                )?;
                if config.gear_message() == "ACCELERATOR" {
                    inputs.capture((Channel::Pt, "ACCELERATOR", "accelerator"), true, now)?;
                }
                for (name, key) in [
                    ("BLINKERS", "blinkers"),
                    ("BLINKERS_ALT", "blinkers_alt"),
                    ("DOORS_SEATBELTS", "doors_seatbelts"),
                ] {
                    inputs.capture((Channel::Pt, name, key), false, now)?;
                }
            }
            126 => {
                inputs.capture(
                    (Channel::Pt, "CRUISE_BUTTONS_ALT2", "cruise_buttons_alt2"),
                    true,
                    now,
                )?;
                inputs.capture((Channel::Pt, "TRAILER_STATUS", "trailer_status"), true, now)?;
            }
            _ => {}
        }
        Ok(())
    }
}
