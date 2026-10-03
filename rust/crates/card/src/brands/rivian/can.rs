use super::Error;
use crate::brands::hyundai::wire::crc8;
use num_traits::ToPrimitive;
use openpilot_can::{packer::Packer, Frame};
use std::collections::BTreeMap;

type Values = BTreeMap<String, f64>;
fn copy<'a>(source: &Values, names: &'a [&'a str]) -> Result<Vec<(&'a str, f64)>, Error> {
    names
        .iter()
        .map(|name| {
            Ok((
                *name,
                source
                    .get(*name)
                    .copied()
                    .ok_or_else(|| Error::Signal((*name).to_owned()))?,
            ))
        })
        .collect()
}
fn checked<'a>(
    packer: &mut Packer,
    name: &str,
    bus: u8,
    mut values: Vec<(&'a str, f64)>,
    checksum: &'a str,
    xor: u8,
) -> Result<Frame, Error> {
    let data = packer.pack(name, &values, None)?;
    let value = f64::from(crc8(&data[1..], 0x1d, 0, xor));
    if let Some((_, target)) = values.iter_mut().find(|(key, _)| *key == checksum) {
        *target = value;
    } else {
        values.push((checksum, value));
    }
    Ok(Frame {
        address: packer.dbc.message(name)?.address,
        data: packer.pack(name, &values, None)?,
        bus,
    })
}
pub fn steering(
    packer: &mut Packer,
    frame: u64,
    source: &Values,
    torque: i32,
    enabled: bool,
    active: bool,
) -> Result<Frame, Error> {
    let mut values = copy(
        source,
        &[
            "ACM_hbaSysState",
            "ACM_hbaLamp",
            "ACM_hbaOnOffState",
            "ACM_slifOnOffState",
        ],
    )?;
    values.extend([
        (
            "ACM_lkaHbaCmd_Counter",
            (frame % 15).to_f64().ok_or(Error::Numeric)?,
        ),
        ("ACM_lkaStrToqReq", f64::from(torque)),
        ("ACM_lkaActToi", f64::from(active)),
        ("ACM_lkaLaneRecogState", if enabled { 3. } else { 0. }),
        ("ACM_lkaSymbolState", if enabled { 3. } else { 0. }),
        ("ACM_lkaElkRequest", 0.),
        ("ACM_ldwlkaOnOffState", 2.),
        ("ACM_elkOnOffState", 1.),
        ("ACM_ldwWarnTypeState", 2.),
        ("ACM_ldwWarnTimingState", 1.),
    ]);
    checked(
        packer,
        "ACM_lkaHbaCmd",
        0,
        values,
        "ACM_lkaHbaCmd_Checksum",
        0x63,
    )
}
pub fn wheel_touch(packer: &mut Packer, source: &Values, enabled: bool) -> Result<Frame, Error> {
    let mut values = copy(
        source,
        &[
            "SCCM_WheelTouch_Counter",
            "SCCM_WheelTouch_HandsOn",
            "SCCM_WheelTouch_CapacitiveValue",
            "SETME_X52",
        ],
    )?;
    if enabled {
        values[1].1 = 1.;
        values[2].1 = 100.;
    }
    checked(
        packer,
        "SCCM_WheelTouch",
        2,
        values,
        "SCCM_WheelTouch_Checksum",
        0x97,
    )
}
pub fn longitudinal(
    packer: &mut Packer,
    frame: u64,
    accel: f64,
    enabled: bool,
) -> Result<Frame, Error> {
    let values = vec![
        (
            "ACM_longitudinalRequest_Counter",
            (frame % 15).to_f64().ok_or(Error::Numeric)?,
        ),
        ("ACM_AccelerationRequest", if enabled { accel } else { 0. }),
        ("ACM_VehicleHoldRequired", 0.),
        ("ACM_PrndRequired", 0.),
        ("ACM_longInterfaceEnable", f64::from(enabled)),
        ("ACM_AccelerationRequestType", 0.),
    ];
    checked(
        packer,
        "ACM_longitudinalRequest",
        0,
        values,
        "ACM_longitudinalRequest_Checksum",
        0x12,
    )
}
pub fn adas(packer: &mut Packer, source: &Values, status: Option<f64>) -> Result<Frame, Error> {
    let mut values = copy(
        source,
        &[
            "VDM_AdasStatus_Checksum",
            "VDM_AdasStatus_Counter",
            "VDM_AdasDecelLimit",
            "VDM_AdasDriverAccelPriorityStatus",
            "VDM_AdasFaultStatus",
            "VDM_AdasAccelLimit",
            "VDM_AdasDriverModeStatus",
            "VDM_AdasAccelRequest",
            "VDM_AdasInterfaceStatus",
            "VDM_AdasAccelRequestAcknowledged",
            "VDM_AdasVehicleHoldStatus",
        ],
    )?;
    if let Some(value) = status {
        values[8].1 = value;
    }
    checked(
        packer,
        "VDM_AdasSts",
        2,
        values,
        "VDM_AdasStatus_Checksum",
        0xd1,
    )
}
