use super::{
    flags as f,
    wire::{boolean, copy_signals, crc8, get, set, values, CanWriter, Values},
    Error,
};
use openpilot_can::Frame;

pub struct LkasInput<'a> {
    pub candidate: &'a str,
    pub flags: u32,
    pub frame: u32,
    pub torque: f64,
    pub steer_req: bool,
    pub torque_fault: bool,
    pub sys_warning: bool,
    pub sys_state: f64,
    pub enabled: bool,
    pub left_lane: bool,
    pub right_lane: bool,
    pub left_depart: f64,
    pub right_depart: f64,
    pub ldws_car: bool,
}

pub fn lkas(
    writer: &mut CanWriter,
    input: &LkasInput<'_>,
    source: &Values,
) -> Result<Frame, Error> {
    let i = input;
    let mut data = copy_signals(
        source,
        &[
            "CF_Lkas_LdwsActivemode",
            "CF_Lkas_LdwsSysState",
            "CF_Lkas_SysWarning",
            "CF_Lkas_LdwsLHWarning",
            "CF_Lkas_LdwsRHWarning",
            "CF_Lkas_HbaLamp",
            "CF_Lkas_FcwBasReq",
            "CF_Lkas_HbaSysState",
            "CF_Lkas_FcwOpt",
            "CF_Lkas_HbaOpt",
            "CF_Lkas_FcwSysState",
            "CF_Lkas_FcwCollisionWarning",
            "CF_Lkas_FusionState",
            "CF_Lkas_FcwOpt_USM",
            "CF_Lkas_LdwsOpt_USM",
        ],
    )?;
    set(
        &mut data,
        &[
            ("CF_Lkas_LdwsSysState", i.sys_state),
            ("CF_Lkas_SysWarning", 0.),
            ("CF_Lkas_LdwsLHWarning", i.left_depart),
            ("CF_Lkas_LdwsRHWarning", i.right_depart),
            ("CR_Lkas_StrToqReq", i.torque),
            ("CF_Lkas_ActToi", boolean(i.steer_req)),
            ("CF_Lkas_ToiFlt", boolean(i.torque_fault)),
            ("CF_Lkas_MsgCount", f64::from(i.frame % 16)),
        ],
    );
    if i.flags & f::SEND_LFA != 0 || i.candidate == "HYUNDAI_SANTA_FE" {
        set(
            &mut data,
            &[
                (
                    "CF_Lkas_LdwsActivemode",
                    boolean(i.left_lane) + 2. * boolean(i.right_lane),
                ),
                (
                    "CF_Lkas_LdwsOpt_USM",
                    if i.candidate == "KIA_RAY_EV" { 0. } else { 2. },
                ),
                ("CF_Lkas_FcwOpt_USM", if i.enabled { 2. } else { 1. }),
                ("CF_Lkas_SysWarning", 0.),
            ],
        );
    } else {
        match i.candidate {
            "KIA_OPTIMA_G4" | "KIA_OPTIMA_G4_FL" => set(
                &mut data,
                &[
                    ("CF_Lkas_SysWarning", if i.sys_warning { 4. } else { 0. }),
                    ("CF_Lkas_LdwsSysState", if i.enabled { 3. } else { 1. }),
                    ("CF_Lkas_LdwsOpt_USM", 2.),
                    ("CF_Lkas_LdwsActivemode", 0.),
                    ("CF_Lkas_FcwOpt_USM", 0.),
                ],
            ),
            "HYUNDAI_GENESIS" => {
                data.insert("CF_Lkas_LdwsActivemode".into(), 2.);
            }
            _ => {}
        }
    }
    if i.ldws_car {
        data.insert("CF_Lkas_LdwsOpt_USM".into(), 3.);
    }
    data.insert("CF_Lkas_Chksum".into(), 0.);
    let frame = writer.frame("LKAS11", 0, &data, None)?;
    let head = frame.data.get(..6).ok_or(Error::Numeric)?;
    let tail = *frame.data.get(7).ok_or(Error::Numeric)?;
    let checksum = if i.flags & f::CHECKSUM_CRC8 != 0 {
        let bytes: Vec<_> = head.iter().copied().chain([tail]).collect();
        crc8(&bytes, 0x1d, 0xfd ^ 0xdf, 0xdf)
    } else {
        head.iter().copied().fold(
            if i.flags & f::CHECKSUM_6B != 0 {
                0
            } else {
                tail
            },
            u8::wrapping_add,
        )
    };
    data.insert("CF_Lkas_Chksum".into(), f64::from(checksum));
    writer.frame("LKAS11", 0, &data, None)
}

pub fn clu_button(
    writer: &mut CanWriter,
    source: &Values,
    flags: u32,
    button: f64,
) -> Result<Frame, Error> {
    let mut data = source.clone();
    let counter = (get(&data, "CF_Clu_AliveCnt1")? + 1.) % 16.;
    set(
        &mut data,
        &[
            ("CF_Clu_CruiseSwState", button),
            ("CF_Clu_AliveCnt1", counter),
        ],
    );
    writer.frame(
        "CLU11",
        if flags & f::CAMERA_SCC != 0 { 2 } else { 0 },
        &data,
        None,
    )
}

pub fn clu_cancel(
    writer: &mut CanWriter,
    source: &Values,
    input: (u32, u32),
) -> Result<Frame, Error> {
    let (flags, frame) = input;
    let mut data = copy_signals(
        source,
        &[
            "CF_Clu_CruiseSwState",
            "CF_Clu_CruiseSwMain",
            "CF_Clu_SldMainSW",
            "CF_Clu_ParityBit1",
            "CF_Clu_VanzDecimal",
            "CF_Clu_Vanz",
            "CF_Clu_SPEED_UNIT",
            "CF_Clu_DetentOut",
            "CF_Clu_RheostatLevel",
            "CF_Clu_CluInfo",
            "CF_Clu_AmpInfo",
            "CF_Clu_AliveCnt1",
        ],
    )?;
    set(
        &mut data,
        &[
            ("CF_Clu_CruiseSwState", 4.),
            ("CF_Clu_AliveCnt1", f64::from(frame % 16)),
        ],
    );
    writer.frame(
        "CLU11",
        if flags & f::CAMERA_SCC != 0 { 2 } else { 0 },
        &data,
        None,
    )
}

pub fn lfa_mfc(writer: &mut CanWriter, active: [bool; 3], carrot: i16) -> Result<Frame, Error> {
    writer.frame(
        "LFAHDA_MFC",
        0,
        &values(&[
            (
                "LFA_Icon_State",
                if active[0] {
                    2.
                } else if active[1] {
                    1.
                } else {
                    0.
                },
            ),
            (
                "HDA_Icon_State",
                if carrot == 3 && active[2] {
                    0.
                } else if carrot >= 1 {
                    2.
                } else {
                    0.
                },
            ),
            ("HDA_VSetReq", 0.),
            ("HDA_USM", 2.),
            ("HDA_Icon_Wheel", if active[0] { 1. } else { 0. }),
        ]),
        None,
    )
}

pub fn mdps(writer: &mut CanWriter, source: &mut Values, frame: u32) -> Result<Frame, Error> {
    set(
        source,
        &[
            ("CF_Mdps_ToiActive", 0.),
            ("CF_Mdps_ToiUnavail", 1.),
            ("CF_Mdps_MsgCount2", f64::from(frame % 256)),
            ("CF_Mdps_Chksum2", 0.),
        ],
    );
    let first = writer.frame("MDPS12", 2, source, None)?;
    let checksum = first.data.iter().copied().fold(0u8, u8::wrapping_add);
    source.insert("CF_Mdps_Chksum2".into(), f64::from(checksum));
    writer.frame("MDPS12", 2, source, None)
}
