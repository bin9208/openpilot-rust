// Source-derived native port of opendbc/dbc/generator (MIT).
// Copyright (c) 2020, Comma.ai, Inc.; see LICENSE.opendbc.
use std::io::{self, Write};
fn hex_i64(value: i64) -> String {
    if value < 0 {
        format!("-{:x}", value.unsigned_abs())
    } else {
        format!("{value:x}")
    }
}
pub(super) fn tesla_point_to(out: &mut dyn Write, base_id: i64, base_name: &str) -> io::Result<()> {
    out.write_all(
        (format!(
            r###"
BO_ {} {}_A: 8 Radar
 SG_ LongDist : 0|12@1+ (0.0625,0) [0|255.9] "meters"  Autopilot
 SG_ LongSpeed : 12|12@1+ (0.0625,-128) [-128|128] "meters/sec"  Autopilot
 SG_ LatDist : 24|11@1+ (0.125,-128) [-128|128] "meters"  Autopilot
 SG_ ProbExist : 35|5@1+ (3.125,0) [0|96.875] "%"  Autopilot
 SG_ LongAccel : 40|10@1+ (0.03125,-16) [-16|16] "meters/sec/sec"  Autopilot
 SG_ ProbObstacle : 50|5@1+ (3.125,0) [0|96.875] "%"  Autopilot
 SG_ Valid : 55|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ ProbNonObstacle : 56|5@1+ (3.125,0) [0|96.875] "%"  Autopilot
 SG_ Meas : 61|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ Tracked : 62|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ Index : 63|1@1+ (1,0) [0|1] ""  Autopilot

BO_ {} {}_B: 8 Radar
 SG_ LatSpeed : 0|10@1+ (0.125,-64) [-64|64] "meters/sec"  Autopilot
 SG_ Length : 10|6@1+ (0.125,0) [0|7.875] "m"  Autopilot
 SG_ dZ : 16|6@1+ (0.25,-5) [-5|10.75] "m"  Autopilot
 SG_ MovingState : 22|2@1+ (1,0) [0|3] ""  Autopilot
 SG_ dxSigma : 24|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ vxSigma : 30|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ axSigma : 36|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ dySigma : 42|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ ProbClass : 48|5@1+ (3.125,0) [0|96.875] "%"  Autopilot
 SG_ Class : 53|3@1+ (1,0) [0|7] ""  Autopilot
 SG_ dxRearEndLoss : 56|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ NotUsed : 62|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ Index2 : 63|1@1+ (1,0) [0|1] ""  Autopilot
"###,
            base_id,
            base_name,
            base_id + 1,
            base_name
        ))
        .as_bytes(),
    )?;
    Ok(())
}
pub(super) fn tesla_values_to(out: &mut dyn Write, base_id: i64) -> io::Result<()> {
    out.write_all((format!(r###"
VAL_ {} MovingState 3 "RADAR_MOVESTATE_STANDING" 2 "RADAR_MOVESTATE_STOPPED" 1 "RADAR_MOVESTATE_MOVING" 0 "RADAR_MOVESTATE_INDETERMINATE" ;
VAL_ {} Class 4 "RADAR_CLASS_CONSTRUCTION_ELEMENT" 3 "RADAR_CLASS_MOVING_PEDESTRIAN" 2 "RADAR_CLASS_MOVING_TWO_WHEEL_VEHICLE" 1 "RADAR_CLASS_MOVING_FOUR_WHEEL_VEHICLE" 0 "RADAR_CLASS_UNKNOWN" ;"###, base_id + 1, base_id + 1)).as_bytes())?;
    Ok(())
}
pub(super) fn corner_180_to(out: &mut dyn Write, addr: i64) -> io::Result<()> {
    let name = format!(r###"CORNER_RADAR_180_OBJECTS_{}"###, hex_i64(addr));
    out.write_all(
        (format!(
            r###"
BO_ {} {}: 32 RADAR
 SG_ SLOT1_QUAL_LEVEL : 24|7@1+ (1,0) [0|100] "%" XXX
 SG_ SLOT1_AGE : 32|8@1+ (1,0) [0|255] "" XXX
 SG_ SLOT1_OBJECT_ID : 44|7@1+ (1,0) [0|127] "" XXX
 SG_ SLOT1_REL_POS_X : 64|13@1+ (0.05,0) [0|409.55] "m" XXX
 SG_ SLOT1_REL_POS_Y : 78|12@1+ (0.05,-102.4) [-102.4|102.35] "m" XXX
 SG_ SLOT1_REL_VEL_X : 91|12@1+ (0.05,-100) [-100|104.75] "m/s" XXX
 SG_ SLOT1_REL_VEL_Y : 104|10@1+ (0.05,-25) [-25|26.15] "m/s" XXX
 SG_ SLOT1_REL_ACCEL_X : 115|9@1- (0.05,0) [-12.8|12.75] "m/s^2" XXX
 SG_ SLOT2_QUAL_LEVEL : 152|7@1+ (1,0) [0|100] "%" XXX
 SG_ SLOT2_AGE : 160|8@1+ (1,0) [0|255] "" XXX
 SG_ SLOT2_OBJECT_ID : 172|7@1+ (1,0) [0|127] "" XXX
 SG_ SLOT2_REL_POS_X : 192|13@1+ (0.05,0) [0|409.55] "m" XXX
 SG_ SLOT2_REL_POS_Y : 206|12@1+ (0.05,-102.4) [-102.4|102.35] "m" XXX
 SG_ SLOT2_REL_VEL_X : 219|12@1+ (0.05,-100) [-100|104.75] "m/s" XXX
 SG_ SLOT2_REL_VEL_Y : 232|10@1+ (0.05,-25) [-25|26.15] "m/s" XXX
 SG_ SLOT2_REL_ACCEL_X : 243|9@1- (0.05,0) [-12.8|12.75] "m/s^2" XXX
"###,
            addr, name
        ))
        .as_bytes(),
    )?;
    out.write_all((format!(r###"CM_ BO_ {} "Corner radar object payload on bus 1. Only signals consumed by Hyundai radar_interface are defined.";
BA_ "GenMsgCycleTime" BO_ {} 30;
"###, addr, addr)).as_bytes())?;
    Ok(())
}
pub(super) fn corner_235_to(out: &mut dyn Write, addr: i64) -> io::Result<()> {
    let name = format!(r###"CORNER_RADAR_235_OBJECTS_{}"###, hex_i64(addr));
    out.write_all(
        (format!(
            r###"
BO_ {} {}: 32 RADAR
 SG_ OBJ_QUAL_LEVEL : 24|7@1+ (1,0) [0|100] "%" XXX
 SG_ OBJ_AGE : 32|8@1+ (1,0) [0|255] "" XXX
 SG_ OBJ_OBJECT_ID : 44|7@1+ (1,0) [0|127] "" XXX
 SG_ OBJ_REL_POS_X : 64|13@1+ (0.05,0) [0|409.55] "m" XXX
 SG_ OBJ_REL_POS_Y : 78|12@1+ (0.05,-102.4) [-102.4|102.35] "m" XXX
 SG_ OBJ_REL_VEL_X : 91|12@1+ (0.05,-100) [-100|104.75] "m/s" XXX
 SG_ OBJ_REL_VEL_Y : 104|10@1+ (0.05,-25) [-25|26.15] "m/s" XXX
 SG_ OBJ_REL_ACCEL_X : 115|9@1- (0.05,0) [-12.8|12.75] "m/s^2" XXX
"###,
            addr, name
        ))
        .as_bytes(),
    )?;
    out.write_all((format!(r###"CM_ BO_ {} "Corner radar object payload on bus 1. Only signals consumed by Hyundai radar_interface are defined.";
BA_ "GenMsgCycleTime" BO_ {} 30;
"###, addr, addr)).as_bytes())?;
    Ok(())
}
pub(super) fn corner_430_to(out: &mut dyn Write, addr: i64) -> io::Result<()> {
    let name = format!(r###"CORNER_RADAR_430_OBJECTS_{}"###, hex_i64(addr));
    out.write_all(
        (format!(
            r###"
BO_ {} {}: 32 RADAR
 SG_ HEADER_BYTE_0 : 0|8@1+ (1,0) [0|255] "" XXX
 SG_ HEADER_BYTE_1 : 8|8@1+ (1,0) [0|255] "" XXX
 SG_ HEADER_BYTE_2 : 16|8@1+ (1,0) [0|255] "" XXX
 SG_ HEADER_BYTE_3 : 24|8@1+ (1,0) [0|255] "" XXX
"###,
            addr, name
        ))
        .as_bytes(),
    )?;
    for slot in 0..7 {
        let bit = 32 + slot * 32;
        let prefix = format!(r###"SLOT{}"###, slot + 1);
        out.write_all(
            (format!(
                r###" SG_ {}_DISTANCE_RAW : {}|13@1+ (1,0) [0|8191] "" XXX
 SG_ {}_META_13_15 : {}|3@1+ (1,0) [0|7] "" XXX
 SG_ {}_META_BYTE_2 : {}|8@1+ (1,0) [0|255] "" XXX
 SG_ {}_META_BYTE_3 : {}|8@1+ (1,0) [0|255] "" XXX
"###,
                prefix,
                bit,
                prefix,
                bit + 13,
                prefix,
                bit + 16,
                prefix,
                bit + 24
            ))
            .as_bytes(),
        )?;
    }
    out.write_all((format!(r###"CM_ BO_ {} "IONIQ 9 candidate corner radar bin container. 0x430-0x437 correlate with left side and 0x440-0x447 with right side. Bytes 4-31 appear as seven 4-byte slots; lower 13 bits are a distance candidate at 0.05 m scale. This DBC intentionally exposes only raw candidate fields until lateral and velocity fields are validated.";
BA_ "GenMsgCycleTime" BO_ {} 30;
"###, addr, addr)).as_bytes())?;
    Ok(())
}
pub(super) fn tesla_radar_bosch(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(
        r###"
VERSION ""

NS_ :
	NS_DESC_
	CM_
	BA_DEF_
	BA_
	VAL_
	CAT_DEF_
	CAT_
	FILTER
	BA_DEF_DEF_
	EV_DATA_
	ENVVAR_DATA_
	SGTYPE_
	SGTYPE_VAL_
	BA_DEF_SGTYPE_
	BA_SGTYPE_
	SIG_TYPE_REF_
	VAL_TABLE_
	SIG_GROUP_
	SIG_VALTYPE_
	SIGTYPE_VALTYPE_
	BO_TX_BU_
	BA_DEF_REL_
	BA_REL_
	BA_DEF_DEF_REL_
	BU_SG_REL_
	BU_EV_REL_
	BU_BO_REL_
	SG_MUL_VAL_

BS_:

BU_:  Autopilot Radar Diag


BO_ 769 TeslaRadarSguInfo: 8 Radar
 SG_ RADC_VerticalMisalignment : 0|8@1+ (1,0) [0|255] ""  Autopilot
 SG_ RADC_SCUTemperature : 8|8@1+ (1,-128) [-128|127] ""  Autopilot
 SG_ RADC_VMA_Plaus : 16|8@1+ (1,0) [0|255] ""  Autopilot
 SG_ RADC_SGU_ITC : 24|8@1+ (1,0) [0|255] ""  Autopilot
 SG_ RADC_HorizontMisalignment : 32|12@1+ (1,0) [0|4096] ""  Autopilot
 SG_ RADC_SensorDirty : 44|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_HWFail : 45|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_SGUFail : 46|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_SGUInfoConsistBit : 47|1@1+ (1,0) [0|1] ""  Autopilot

BO_ 770 TeslaRadarTguInfo: 8 Radar
 SG_ RADC_ACCTargObj1_sguIndex : 0|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ RADC_ACCTargObj2_sguIndex : 6|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ RADC_ACCTargObj3_sguIndex : 12|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ RADC_ACCTargObj4_sguIndex : 18|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ RADC_ACCTargObj5_sguIndex : 24|6@1+ (1,0) [0|63] ""  Autopilot
 SG_ unused30 : 30|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_TGUInfoConsistBit : 31|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_ACCTargObj1_dBPower : 32|16@1+ (1,0) [0|65535] ""  Autopilot
 SG_ RADC_ACCTargObj5_dBPower : 48|16@1+ (1,0) [0|65535] ""  Autopilot

BO_ 1281 TeslaRadarAlertMatrix: 8 Radar
 SG_ RADC_a001_ecuInternalPerf : 0|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a002_flashPerformance : 1|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a003_vBatHigh : 2|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a004_adjustmentNotDone : 3|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a005_adjustmentReq : 4|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a006_adjustmentNotOk : 5|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a007_sensorBlinded : 6|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a008_plantModeActive : 7|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a009_configMismatch : 8|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a010_canBusOff : 9|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a011_bdyMIA : 10|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a012_espMIA : 11|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a013_gtwMIA : 12|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a014_sccmMIA : 13|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a015_adasMIA : 14|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a016_bdyInvalidCount : 15|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a017_adasInvalidCount : 16|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a018_espInvalidCount : 17|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a019_sccmInvalidCount : 18|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a020_bdyInvalidChkSm : 19|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a021_espInvalidChkSm : 20|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a022_sccmInvalidChkSm : 21|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a023_sccmInvalidChkSm : 22|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a024_absValidity : 23|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a025_ambTValidity : 24|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a026_brakeValidity : 25|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a027_CntryCdValidity : 26|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a028_espValidity : 27|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a029_longAccOffValidity : 28|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a030_longAccValidity : 29|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a031_odoValidity : 30|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a032_gearValidity : 31|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a033_steerAngValidity : 32|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a034_steerAngSpdValidity : 33|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a035_indctrValidity : 34|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a036_vehStandStillValidity : 35|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a037_vinValidity : 36|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a038_whlRotValidity : 37|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a039_whlSpdValidity : 38|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a040_whlStandStillValidity : 39|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a041_wiperValidity : 40|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a042_xwdValidity : 41|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a043_yawOffValidity : 42|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a044_yawValidity : 43|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a045_bsdSanity : 44|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a046_rctaSanity : 45|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a047_lcwSanity : 46|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a048_steerAngOffSanity : 47|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a049_tireSizeSanity : 48|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a050_velocitySanity : 49|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a051_yawSanity : 50|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a052_radomeHtrInop : 51|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a053_espmodValidity : 52|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a054_gtwmodValidity : 53|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a055_stwmodValidity : 54|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a056_bcmodValidity : 55|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a057_dimodValidity : 56|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a058_opmodValidity : 57|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a059_drmiInvalidChkSm : 58|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a060_drmiInvalidCount : 59|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a061_radPositionMismatch : 60|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ RADC_a062_strRackMismatch : 61|1@1+ (1,0) [0|1] ""  Autopilot
 SG_ unused62 : 62|2@1+ (1,0) [0|3] ""  Autopilot
"###
        .as_bytes(),
    )?;
    let m_range = (784..877 + 1).step_by(3);
    for (i, base_id) in m_range.clone().enumerate().map(|(i, v)| (i as i64, v)) {
        tesla_point_to(out, base_id, &format!(r###"RadarPoint{}"###, i))?;
    }
    let l_range = (881..893 + 1).step_by(3);
    for (i, base_id) in l_range.clone().enumerate().map(|(i, v)| (i as i64, v)) {
        tesla_point_to(
            out,
            base_id,
            &format!(r###"ProcessedRadarPoint{}"###, i + 1),
        )?;
    }
    out.write_all(r###"
BO_ 697 VIN_VIP_405HS: 8 Autopilot
 SG_ VIN_MuxID M : 0|8@1+ (1,0) [0|0] ""  Radar
 SG_ VIN_Part1 m16 : 47|24@0+ (1,0) [0|16777215] "" Radar
 SG_ VIN_Part2 m17 : 15|56@0+ (1,0) [0|7.2057594038E+16] "" Radar
 SG_ VIN_Part3 m18 : 15|56@0+ (1,0) [0|7.2057594038E+16] "" Radar

BO_ 681 Msg2A9_GTW_carConfig: 8 Autopilot
 SG_ Msg2A9_Always0x02 : 48|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg2A9_Always0x10 : 56|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg2A9_Always0x16 : 8|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg2A9_Always0x41 : 24|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg2A9_Value1_0x02 : 0|3@1+ (1,0) [0|0] "" Radar
 SG_ Msg2A9_FourWheelDrive : 3|2@1+ (1,0) [0|0] "" Radar
 SG_ Msg2A9_Value2_0x02 : 5|3@1+ (1,0) [0|0] ""  Radar
 SG_ Msg2A9_Always0x43 : 16|8@1+ (1,0) [0|0] ""  Radar

BO_ 409 Msg199_STW_ANGLHP_STAT: 8 Autopilot
 SG_ Msg199Always0x04 : 32|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg199Always0x20 : 16|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg199Always0x2F : 0|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg199Always0x67 : 8|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg199Always0xFF : 40|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg199Checksum : 56|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg199Counter : 52|4@1+ (1,0) [0|0] ""  Radar

BO_ 361 Msg169_ESP_wheelSpeeds: 8 Autopilot
 SG_ ESP_wheelSpeedFrL_HS : 0|13@1+ (0.04,0) [0|327.64] "km/h" Radar
 SG_ ESP_wheelSpeedFrR_HS : 13|13@1+ (0.04,0) [0|327.64] "km/h" Radar
 SG_ ESP_wheelSpeedReL_HS : 26|13@1+ (0.04,0) [0|327.64] "km/h" Radar
 SG_ ESP_wheelSpeedReR_HS : 39|13@1+ (0.04,0) [0|327.64] "km/h" Radar
 SG_ Msg169Checksum : 56|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg169Counter : 52|4@1+ (1,0) [0|0] ""  Radar

BO_ 345 Msg159_ESP_C: 8 Autopilot
 SG_ Msg159Always0x3A : 16|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg159Always0xA5 : 0|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg159Always0xCF : 32|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg159Always0xF4 : 8|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg159Counter : 44|4@1+ (1,0) [0|0] ""  Radar
 SG_ Msg159Checksum : 24|8@1+ (1,0) [0|0] ""  Radar

BO_ 329 Msg149_ESP_145h: 8 Autopilot
 SG_ Msg149Always0x02 : 16|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg149Always0x04 : 40|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg149Always0x26 : 8|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg149Always0x6A : 24|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg149Always0xAA : 32|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg149Always0xF : 48|4@1+ (1,0) [0|0] ""  Radar
 SG_ Msg149Checksum : 56|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg149Counter : 52|4@1+ (1,0) [0|0] ""  Radar

BO_ 297 Msg129_ESP_115h: 6 Autopilot
 SG_ Msg129Always0x20 : 24|8@1+ (1,0) [0|0] "" Radar
 SG_ Msg129Checksum : 40|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg129Counter : 36|4@1+ (1,0) [0|0] ""  Radar

BO_ 281 Msg119_DI_torque2: 6 Autopilot
 SG_ Msg119Always0x11 : 24|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg119Always0x1F : 8|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg119Always0x8 : 36|4@1+ (1,0) [0|0] ""  Radar
 SG_ Msg119Always0xF4 : 16|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg119Always0xFF : 0|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg119Checksum : 40|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg119Counter : 32|4@1+ (1,0) [0|0] ""  Radar

BO_ 265 Msg109_DI_torque1: 8 Autopilot
 SG_ Msg109Always0x80 : 24|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg109Checksum : 56|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg109Counter : 13|3@1+ (1,0) [0|0] ""  Radar

BO_ 521 Msg209_GTW_odo: 8 Autopilot
 SG_ Msg209Always0x61 : 8|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg209Always0x94 : 16|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg209Always0x52 : 24|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg209Always0x13 : 32|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg209Always0x03 : 40|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg209Always0x80 : 48|8@1+ (1,0) [0|0] ""  Radar

BO_ 537 Msg219_STW_ACTN_RQ: 8 Autopilot
 SG_ Msg219Counter : 52|4@1+ (1,0) [0|15] "" Radar
 SG_ Msg219CRC : 56|8@1+ (1,0) [0|0] "" Radar

BO_ 425 Msg1A9_DI_espControl: 5 Autopilot
 SG_ Msg1A9Always0x0C : 16|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg1A9Counter : 28|4@1+ (1,0) [0|0] ""  Radar
 SG_ Msg1A9Checksum : 32|8@1+ (1,0) [0|0] ""  Radar

BO_ 729 Msg2D9_BC_status : 8 Autopilot
 SG_ Msg2D9Always0x80 : 0|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg2D9Always0x40 : 8|8@1+ (1,0) [0|0] ""  Radar
 SG_ Msg2D9Always0x83 : 16|8@1+ (1,0) [0|0] ""  Radar

BO_ 1601 UDS_radarRequest: 8 Diag
 SG_ UDS_radarRequestData : 7|64@0+ (1,0) [0|0] "" Radar

BO_ 1617 Radar_udsResponse: 8 Radar
 SG_ Radar_udsResponseData : 7|64@0+ (1,0) [0|0] "" Diag

CM_ BO_ 697 "Start with MuxID 0x12, then 0x11 and finally 0x10 (VIN is then transmitted in the reverse order)";
CM_ BO_ 681 "Message sent every 1000 ms. All fixed bytes, no checksum, the byte for RWD or AWD needs to match VIN config";
CM_ BO_ 409 "Message sent every 10ms. Checksum : use all first 7 bytes with the SAE J1850 CRC algo";
CM_ BO_ 361 "Message sent every 10ms. Checksum : Sum of all first 7 bytes + 0x76";
CM_ BO_ 345 "Message sent every 20ms. Checksum : Sum of all first bytes + 0xc; place checksum in 4th octet";
CM_ BO_ 329 "Message sent every 20ms. Checksum : Sum of all first 7 bytes + 0x46";
CM_ BO_ 297 "Message sent every 20ms. Checksum : Sum of all first 5 bytes + 0x16";
CM_ BO_ 281 "Message sent every 10ms. Checksum : Sum of all first 5 bytes + 0x17";
CM_ BO_ 265 "Message sent every 10ms. Checksum : Sum of all first 7 bytes + 0x7";
CM_ BO_ 521 "Message sent every 100ms. All fixed bytes, no checksum.";
CM_ BO_ 537 "Message sent every 100ms. Checksum : use all first 7 bytes with the SAE J1850 CRC algo";
CM_ BO_ 425 "Message sent every 20ms. Checksum : Sum of all first 4 bytes + 0x38";
CM_ BO_ 729 "Message sent every 1000ms. All fixed bytes, no checksum.";

BA_DEF_ "BusType" STRING ;
BA_DEF_ BO_ "GenMsgCycleTime" INT 0 0;
BA_DEF_ SG_ "FieldType" STRING ;

BA_DEF_DEF_ "BusType" "CAN";
BA_DEF_DEF_ "FieldType" "";
BA_DEF_DEF_ "GenMsgCycleTime" 0;

BA_ "GenMsgCycleTime" BO_ 697 250;
BA_ "GenMsgCycleTime" BO_ 681 1000;
BA_ "GenMsgCycleTime" BO_ 409 10;
BA_ "GenMsgCycleTime" BO_ 361 10;
BA_ "GenMsgCycleTime" BO_ 345 20;
BA_ "GenMsgCycleTime" BO_ 329 20;
BA_ "GenMsgCycleTime" BO_ 297 20;
BA_ "GenMsgCycleTime" BO_ 281 10;
BA_ "GenMsgCycleTime" BO_ 265 10;
BA_ "GenMsgCycleTime" BO_ 521 100;
BA_ "GenMsgCycleTime" BO_ 537 100;
BA_ "GenMsgCycleTime" BO_ 425 20;
BA_ "GenMsgCycleTime" BO_ 729 1000;

VAL_ 681 Msg2A9_FourWheelDrive 3 "SNA" 2 "UNUSED" 1 "4WD" 0 "2WD" ;"###.as_bytes())?;
    for base_id in m_range.clone().chain(l_range.clone()) {
        tesla_values_to(out, base_id)?;
    }
    Ok(())
}
pub(super) fn tesla_radar_continental(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(
        r###"
VERSION ""

NS_ :
	NS_DESC_
	CM_
	BA_DEF_
	BA_
	VAL_
	CAT_DEF_
	CAT_
	FILTER
	BA_DEF_DEF_
	EV_DATA_
	ENVVAR_DATA_
	SGTYPE_
	SGTYPE_VAL_
	BA_DEF_SGTYPE_
	BA_SGTYPE_
	SIG_TYPE_REF_
	VAL_TABLE_
	SIG_GROUP_
	SIG_VALTYPE_
	SIGTYPE_VALTYPE_
	BO_TX_BU_
	BA_DEF_REL_
	BA_REL_
	BA_DEF_DEF_REL_
	BU_SG_REL_
	BU_EV_REL_
	BU_BO_REL_
	SG_MUL_VAL_

BS_:

BU_:  Autopilot Radar Diag

BO_ 1025 RadarStatus: 8 Radar
   SG_ carparkDetected : 29|1@1+ (1,0) [0|1] "" Autopilot
   SG_ decreaseBlockage : 25|1@1+ (1,0) [0|1] "" Autopilot
   SG_ horizontMisalignment : 8|12@1+ (0.00012207,-0.25) [-0.25|0.249878] "rad" Autopilot
   SG_ increaseBlockage : 24|1@1+ (1,0) [0|1] "" Autopilot
   SG_ lowPowerMode : 20|2@1+ (1,0) [0|3] "" Autopilot
   SG_ powerOnSelfTest : 22|1@1+ (1,0) [0|1] "" Autopilot
   SG_ sensorBlocked : 26|1@1+ (1,0) [0|1] "" Autopilot
   SG_ sensorInfoConsistBit : 30|1@1+ (1,0) [0|1] "" Autopilot
   SG_ sensorReplace : 31|1@1+ (1,0) [0|1] "" Autopilot
   SG_ shortTermUnavailable : 23|1@1+ (1,0) [0|1] "" Autopilot
   SG_ tunnelDetected : 28|1@1+ (1,0) [0|1] "" Autopilot
   SG_ vehDynamicsError : 27|1@1+ (1,0) [0|1] "" Autopilot
   SG_ verticalMisalignment : 0|8@1+ (0.00195313,-0.25) [-0.25|0.248047] "rad" Autopilot

BO_ 1617 Radar_udsResponse: 8 Radar
   SG_ Radar_udsResponseData : 7|64@0+ (1,0) [0|1.84467e+19] "" Diag

BO_ 1601 UDS_radcRequest: 8 Diag
   SG_ UDS_radcRequestData : 7|64@0+ (1,0) [0|1.84467e+19] "" Radar
"###
        .as_bytes(),
    )?;
    let point_range = (1040..1118 + 1).step_by(2);
    for (i, base_id) in point_range.clone().enumerate().map(|(i, v)| (i as i64, v)) {
        tesla_point_to(out, base_id, &format!(r###"RadarPoint{}"###, i))?;
    }
    out.write_all(
        r###"
VAL_ 1025 lowPowerMode 1 "COMMANDED_LOW_POWER" 0 "DEFAULT_LOW_POWER" 2 "NORMAL_POWER" 3 "SNA";"###
            .as_bytes(),
    )?;
    for base_id in point_range.clone() {
        tesla_values_to(out, base_id)?;
    }
    Ok(())
}
pub(super) fn hyundai_canfd_corner_radar_235(out: &mut dyn Write) -> io::Result<()> {
    let header = r###"VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX RADAR

"###;
    out.write_all(header.as_bytes())?;
    out.write_all(
        r###"BA_DEF_ BO_  "GenMsgCycleTime" INT 0 100000;
"###
        .as_bytes(),
    )?;
    out.write_all(
        r###"BA_DEF_DEF_  "GenMsgCycleTime" 0;
"###
        .as_bytes(),
    )?;
    for addr in 565..585 {
        corner_235_to(out, addr)?;
    }
    Ok(())
}
pub(super) fn hyundai_canfd_radar(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(
        r###"
VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    VAL_TABLE_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX
    "###
        .as_bytes(),
    )?;
    for a in 528..528 + 16 {
        out.write_all(
            (format!(
                r###"
BO_ {} RADAR_TRACK_{}: 32 RADAR
 SG_ NEW_SIGNAL_25 : 26|3@0+ (1,0) [0|7] "" XXX
 SG_ NEW_SIGNAL_24 : 28|2@0+ (1,0) [0|3] "" XXX
 SG_ NEW_SIGNAL_21 : 36|5@0+ (1,0) [0|31] "" XXX
 SG_ NEW_SIGNAL_20 : 39|3@0+ (1,0) [0|7] "" XXX
 SG_ VALID_CNT1 : 47|8@0+ (1,0) [0|255] "" XXX
 SG_ NEW_SIGNAL_15 : 51|4@0+ (1,0) [0|15] "" XXX
 SG_ NEW_SIGNAL_14 : 55|4@0+ (1,0) [0|15] "" XXX
 SG_ NEW_SIGNAL_5 : 63|8@0- (1,0) [0|255] "" XXX
 SG_ LONG_DIST1 : 64|12@1+ (0.05,0) [0|4095] "" XXX
 SG_ LAT_DIST1 : 76|12@1- (0.05,0) [0|4095] "" XXX
 SG_ REL_SPEED1 : 88|14@1- (0.01,0) [0|16383] "" XXX
 SG_ NEW_SIGNAL_16 : 103|2@0+ (1,0) [0|3] "" XXX
 SG_ LAT_SPEED1 : 104|13@1- (0.01,0) [0|8191] "" XXX
 SG_ REL_ACCEL1 : 118|10@1- (0.05,0) [0|1023] "" XXX
 SG_ NEW_SIGNAL_27 : 154|3@0+ (1,0) [0|7] "" XXX
 SG_ NEW_SIGNAL_26 : 156|2@0+ (1,0) [0|3] "" XXX
 SG_ NEW_SIGNAL_23 : 164|5@0+ (1,0) [0|31] "" XXX
 SG_ NEW_SIGNAL_22 : 167|3@0+ (1,0) [0|7] "" XXX
 SG_ VALID_CNT2 : 175|8@0+ (1,0) [0|255] "" XXX
 SG_ NEW_SIGNAL_13 : 179|4@0+ (1,0) [0|15] "" XXX
 SG_ NEW_SIGNAL_12 : 183|4@0+ (1,0) [0|15] "" XXX
 SG_ NEW_SIGNAL_11 : 191|8@0- (1,0) [0|255] "" XXX
 SG_ LONG_DIST2 : 192|12@1+ (0.05,0) [0|4095] "" XXX
 SG_ LAT_DIST2 : 204|12@1- (0.05,0) [0|4095] "" XXX
 SG_ REL_SPEED2 : 216|14@1- (0.01,0) [0|16383] "" XXX
 SG_ NEW_SIGNAL_17 : 231|2@0+ (1,0) [0|3] "" XXX
 SG_ LAT_SPEED2 : 232|13@1- (0.01,0) [0|8191] "" XXX
 SG_ REL_ACCEL2 : 246|10@1- (0.05,0) [0|1023] "" XXX
    "###,
                a,
                hex_i64(a)
            ))
            .as_bytes(),
        )?;
    }
    for a in 933..933 + 32 {
        out.write_all(
            (format!(
                r###"
BO_ {} RADAR_TRACK_{}: 24 RADAR
 SG_ VALID : 25|2@0+ (1,0) [0|3] "" XXX
 SG_ VALID2 : 28|2@0+ (1,0) [0|3] "" XXX
 SG_ PROB : 30|10@1+ (1,0) [0|1023] "" XXX
 SG_ VALID_CNT : 47|8@0+ (1,0) [0|255] "" XXX
 SG_ NEW_SIGNAL_7 : 51|4@0+ (1,0) [0|15] "" XXX
 SG_ NEW_SIGNAL_6 : 55|4@0+ (1,0) [0|15] "" XXX
 SG_ NEW_SIGNAL_2 : 62|7@0- (1,0) [0|127] "" XXX
 SG_ LONG_DIST : 63|13@1+ (0.05,0) [0|8191] "" XXX
 SG_ LAT_DIST : 76|12@1- (0.05,0) [0|4095] "" XXX
 SG_ REL_SPEED : 88|14@1- (0.01,0) [0|16383] "" XXX
 SG_ IN_MYLANE : 103|2@0+ (1,0) [0|3] "" XXX
 SG_ LAT_SPEED : 104|13@1- (0.01,0) [0|8191] "" XXX
 SG_ REL_ACCEL : 118|10@1- (0.05,0) [0|1023] "" XXX{}
    "###,
                a,
                hex_i64(a),
                " "
            ))
            .as_bytes(),
        )?;
    }
    for a in 1024..1024 + 30 {
        out.write_all(
            (format!(
                r###"
BO_ {} RADAR_TRACK_{}: 24 RADAR
 SG_ OBJECT_ID : 24|7@1+ (1,0) [0|127] "" XXX
 SG_ OBJECT_LENGTH : 41|7@1+ (0.1,0) [0|12.7] "m" XXX
 SG_ LONG_DIST : 64|11@1+ (0.1,0) [0|204.7] "m" XXX
 SG_ LAT_DIST : 75|12@1- (0.05,0) [-102.4|102.35] "m" XXX
 SG_ REL_SPEED : 87|11@1- (0.1,0) [-102.4|102.3] "m/s" XXX
    "###,
                a,
                hex_i64(a)
            ))
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn hyundai_kia_mando_front_radar(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(
        r###"
VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    VAL_TABLE_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX
    "###
        .as_bytes(),
    )?;
    for a in 1280..1280 + 64 {
        out.write_all(
            (format!(
                r###"
BO_ {} RADAR_TRACK_{}: 8 RADAR
 SG_ UNKNOWN_1 : 7|8@0- (1,0) [-128|127] "" XXX
 SG_ AZIMUTH : 12|10@0- (0.2,0) [-102.4|102.2] "" XXX
 SG_ STATE : 15|3@0+ (1,0) [0|7] "" XXX
 SG_ LONG_DIST : 18|11@0+ (0.1,0) [0|204.7] "" XXX
 SG_ REL_ACCEL : 33|10@0- (0.02,0) [-10.24|10.22] "" XXX
 SG_ ZEROS : 37|4@0+ (1,0) [0|255] "" XXX
 SG_ COUNTER : 38|1@0+ (1,0) [0|1] "" XXX
 SG_ STATE_3 : 39|1@0+ (1,0) [0|1] "" XXX
 SG_ REL_SPEED : 53|14@0- (0.01,0) [-81.92|81.92] "" XXX
 SG_ STATE_2 : 55|2@0+ (1,0) [0|3] "" XXX
    "###,
                a,
                hex_i64(a)
            ))
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn hyundai_canfd_corner_radar_430(out: &mut dyn Write) -> io::Result<()> {
    let header = r###"VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX RADAR

"###;
    out.write_all(header.as_bytes())?;
    out.write_all(
        r###"BA_DEF_ BO_  "GenMsgCycleTime" INT 0 100000;
"###
        .as_bytes(),
    )?;
    out.write_all(
        r###"BA_DEF_DEF_  "GenMsgCycleTime" 0;
"###
        .as_bytes(),
    )?;
    for addr in 1072..1080 {
        corner_430_to(out, addr)?;
    }
    for addr in 1088..1096 {
        corner_430_to(out, addr)?;
    }
    Ok(())
}
pub(super) fn hyundai_canfd_corner_radar_180(out: &mut dyn Write) -> io::Result<()> {
    let header = r###"VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    ENVVAR_TYPE_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    VAL_TABLE_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX RADAR

"###;
    out.write_all(header.as_bytes())?;
    out.write_all(
        r###"BA_DEF_ BO_  "GenMsgCycleTime" INT 0 100000;
"###
        .as_bytes(),
    )?;
    out.write_all(
        r###"BA_DEF_DEF_  "GenMsgCycleTime" 0;
"###
        .as_bytes(),
    )?;
    for addr in 384..389 {
        corner_180_to(out, addr)?;
    }
    Ok(())
}
pub(super) fn hyundai_kia_denso_front_radar(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(
        r###"
VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    VAL_TABLE_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX
"###
        .as_bytes(),
    )?;
    for address in 1280..1288 {
        out.write_all(
            (format!(
                r###"
BO_ {} RADAR_TRACK_{}: 8 RADAR
 SG_ COUNTER : 7|8@0+ (1,0) [0|255] "" XXX
 SG_ LONG_DIST : 15|16@0+ (0.00625,0) [0|409.59375] "m" XXX
 SG_ LAT_DIST : 31|11@0- (0.0625,0) [-64|63.9375] "m" XXX
 SG_ OBJECT_FLAGS_1 : 36|5@0+ (1,0) [0|31] "" XXX
 SG_ OBJECT_FLAG_2 : 47|1@0+ (1,0) [0|1] "" XXX
 SG_ REL_SPEED : 46|12@0- (0.015625,0) [-32|31.984375] "m/s" XXX
 SG_ OBJECT_STATE : 50|3@0+ (1,0) [0|7] "" XXX
 SG_ OBJECT_DETAIL : 63|8@0+ (1,0) [0|255] "" XXX
"###,
                address,
                hex_i64(address)
            ))
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn rivian_mando_front_radar(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(
        r###"
VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    VAL_TABLE_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX
    "###
        .as_bytes(),
    )?;
    for a in 1280..1280 + 32 {
        out.write_all(
            (format!(
                r###"
BO_ {} RADAR_TRACK_{}: 8 RADAR
 SG_ CHECKSUM : 0|8@1+ (1,0) [0|255] "" XXX
 SG_ COUNTER : 11|4@0+ (1,0) [0|15] "" XXX
 SG_ UNKNOWN_1 : 23|8@0- (1,0) [-128|127] "" XXX
 SG_ AZIMUTH : 28|10@0- (0.1,0) [-61.2|62.1] "" XXX
 SG_ STATE : 31|3@0+ (1,0) [0|7] "" XXX
 SG_ LONG_DIST : 34|11@0+ (0.1,0) [0|204.7] "" XXX
 SG_ STATE_2 : 55|1@0+ (1,0) [0|1] "" XXX
 SG_ REL_SPEED : 53|14@0- (0.01,0) [-81.92|81.92] "" XXX
    "###,
                a,
                hex_i64(a)
            ))
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) const MANDO_HEADER: &str = r###"
VERSION ""


NS_ :
    NS_DESC_
    CM_
    BA_DEF_
    BA_
    VAL_
    CAT_DEF_
    CAT_
    FILTER
    BA_DEF_DEF_
    EV_DATA_
    ENVVAR_DATA_
    SGTYPE_
    SGTYPE_VAL_
    BA_DEF_SGTYPE_
    BA_SGTYPE_
    SIG_TYPE_REF_
    VAL_TABLE_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_
    SG_MUL_VAL_

BS_:

BU_: XXX
"###;
pub(super) fn mando_metadata(out: &mut dyn Write, a: i64) -> io::Result<()> {
    out.write_all(
        (format!(
            r###"
BO_ {} RADAR_POINTS_METADATA_0x{}: 64 RADAR
 SG_ SIGNAL_1 : 0|32@1+ (1,0) [0|255] "" XXX
 SG_ SIGNAL_2 : 32|32@1+ (1,0) [0|65535] "" XXX
 SG_ SIGNAL_3 : 64|4@1+ (1,0) [0|15] "" XXX
 SG_ SIGNAL_4 : 68|4@1+ (1,0) [0|15] "" XXX
 SG_ RADAR_POINT_COUNT : 72|8@1+ (1,0) [0|255] "" XXX
 SG_ SIGNAL_6 : 80|7@1+ (0.015625,0) [0|3] "" XXX
 SG_ SIGNAL_7 : 87|1@1+ (1,0) [0|1] "" XXX
 SG_ SIGNAL_8 : 88|3@1+ (1,0) [0|7] "" XXX
 SG_ SIGNAL_9 : 91|5@1+ (0.0625,0) [0|31] "" XXX
 SG_ SIGNAL_10 : 96|8@1+ (1,0) [0|255] "" XXX
 SG_ SIGNAL_11 : 104|7@1+ (0.015625,0) [0|127] "" XXX
 SG_ SIGNAL_12 : 111|2@1+ (1,0) [0|65535] "" XXX
 SG_ SIGNAL_13 : 113|7@1+ (0.015625,0) [0|127] "" XXX
 SG_ SIGNAL_14 : 120|7@1+ (0.015625,0) [0|127] "" XXX
 SG_ SIGNAL_15 : 127|3@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_16 : 130|2@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_17 : 133|2@0+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_18 : 134|1@0+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_19 : 135|3@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_20 : 138|8@1+ (1,0) [0|63] "" XXX
 SG_ SIGNAL_21 : 146|2@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_22 : 148|1@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_23 : 149|4@1+ (1,0) [0|7] "" XXX
 SG_ SIGNAL_24 : 153|1@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_25 : 154|2@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_26 : 157|2@0+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_27 : 158|7@1+ (0.125,0) [0|3] "" XXX
 SG_ SIGNAL_28 : 165|7@1+ (0.015625,0) [0|31] "" XXX
 SG_ SIGNAL_29 : 172|7@1+ (0.125,0) [0|3] "" XXX
 SG_ SIGNAL_30 : 179|7@1+ (0.015625,0) [0|1] "" XXX
 SG_ SIGNAL_31 : 186|4@1+ (1,0) [0|7] "" XXX
 SG_ SIGNAL_32 : 190|14@1+ (0.015625,0) [0|15] "" XXX
 SG_ SIGNAL_33 : 204|11@1+ (0.03125,0) [0|8191] "" XXX
 SG_ SIGNAL_34 : 215|2@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_35 : 217|7@1+ (1,0) [0|127] "" XXX
 SG_ SIGNAL_36 : 224|6@1+ (1,0) [0|63] "" XXX
 SG_ SIGNAL_37 : 230|6@1+ (0.2,0) [0|31] "" XXX
 SG_ SIGNAL_38 : 236|6@1+ (0.2,0) [0|7] "" XXX
 SG_ SIGNAL_39 : 242|8@1+ (1,-90) [0|255] "" XXX
 SG_ SIGNAL_40 : 250|6@1+ (1,0) [0|63] "" XXX
 SG_ SIGNAL_41 : 256|8@1+ (0.25,0) [0|255] "" XXX
 SG_ SIGNAL_42 : 264|3@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_43 : 267|12@1+ (0.01,0) [0|31] "" XXX
 SG_ SIGNAL_44 : 279|32@1+ (1,0) [0|63] "" XXX
 SG_ SIGNAL_45 : 311|1@1+ (1,0) [0|1] "" XXX
 SG_ SIGNAL_46 : 312|2@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_47 : 314|32@1+ (1,0) [0|255] "" XXX
 SG_ SIGNAL_48 : 346|6@1+ (1,0) [0|63] "" XXX
 SG_ SIGNAL_49 : 352|7@1+ (0.25,0) [0|127] "" XXX
 SG_ SIGNAL_50 : 359|6@1+ (0.03125,0) [0|31] "" XXX
 SG_ SIGNAL_51 : 365|10@1+ (0.125,0) [0|3] "" XXX
 SG_ SIGNAL_52 : 375|10@1+ (0.125,0) [0|63] "" XXX
 SG_ SIGNAL_53 : 385|7@1+ (1,0) [0|127] "" XXX
 SG_ SIGNAL_54 : 392|7@1+ (1,0) [0|127] "" XXX
 SG_ SIGNAL_55 : 399|8@1+ (0.00390625,0) [0|31] "" XXX
 SG_ SIGNAL_56 : 407|10@1+ (0.125,0) [0|63] "" XXX
 SG_ SIGNAL_57 : 417|1@1+ (1,0) [0|3] "" XXX
 SG_ SIGNAL_58 : 418|1@1+ (1,0) [0|3] "" XXX
"###,
            a,
            hex_i64(a)
        ))
        .as_bytes(),
    )
}
pub(super) fn mando_points(out: &mut dyn Write, a: i64) -> io::Result<()> {
    out.write_all(
        (format!(
            r###"
BO_ {} RADAR_POINTS_0x{}: 64 RADAR
 SG_ MESSAGE_ID : 0|5@1+ (1,0) [0|31] "" XXX
 SG_ LAYOUT_ID : 5|2@1+ (1,0) [0|3] "" XXX
"###,
            a,
            hex_i64(a)
        ))
        .as_bytes(),
    )
}
pub(super) fn mando_checksum(out: &mut dyn Write, a: i64) -> io::Result<()> {
    out.write_all(
        (format!(
            r###"
BO_ {} RADAR_POINTS_CHECKSUM_0x{}: 3 RADAR
 SG_ CRC16 : 0|16@1+ (1,0) [0|65535] "" XXX
"###,
            a,
            hex_i64(a)
        ))
        .as_bytes(),
    )
}
