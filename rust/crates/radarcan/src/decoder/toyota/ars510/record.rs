//! ARS510 0x80 CRC and object-slot extraction.
pub const RECORD_LEN: usize = 742;
pub const IDLE_SLOT: [u8; 36] = [
    252, 224, 0, 0, 160, 240, 127, 0, 255, 253, 247, 31, 255, 161, 0, 248, 7, 0, 15, 0, 8, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
pub fn id80_crc(body: &[u8]) -> u32 {
    crc32fast::hash(body)
}
pub fn id80_crc_ok(record: &[u8]) -> bool {
    if record.len() != RECORD_LEN {
        return false;
    }
    let observed = u32::from_le_bytes(record[737..741].try_into().expect("checked record size"));
    observed == id80_crc(&record[1..737])
}
pub fn slot_bytes(record: &[u8], slot: usize) -> &[u8] {
    let start = 17 + slot * 36;
    record
        .get(start..(start + 36).min(record.len()))
        .unwrap_or(&[])
}
pub fn occupied_slots(record: &[u8]) -> impl Iterator<Item = (usize, &[u8])> {
    (0..20)
        .map(|slot| (slot, slot_bytes(record, slot)))
        .filter(|(_, bytes)| *bytes != IDLE_SLOT)
}
pub fn live_object_count(record: &[u8]) -> u8 {
    (u16::from_le_bytes([
        record.get(14).copied().unwrap_or(0),
        record.get(15).copied().unwrap_or(0),
    ]) >> 3
        & 31) as u8
}
