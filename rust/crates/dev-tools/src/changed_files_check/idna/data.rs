//! Pinned CPython source-literal tables; see provenance.json and lane generator.
pub(super) const B3: &[u8] = include_bytes!("b3.bin");
pub(super) const DECOMP: &[u8] = include_bytes!("decomp.bin");
const VALUES: &[u8] = include_bytes!("values.bin");
const COMPOSE: &[u8] = include_bytes!("compose.bin");
const PROPERTIES: &[u8] = include_bytes!("properties.bin");
fn word(raw: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(raw[at..at + 4].try_into().expect("pinned table u32"))
}
pub(super) fn mapping(table: &[u8], cp: u32, output: &mut Vec<u32>) -> bool {
    let mut first = 0;
    let mut last = table.len() / 12;
    while first < last {
        let mid = first + (last - first) / 2;
        let at = mid * 12;
        match cp.cmp(&word(table, at)) {
            std::cmp::Ordering::Less => last = mid,
            std::cmp::Ordering::Greater => first = mid + 1,
            std::cmp::Ordering::Equal => {
                let start = word(table, at + 4) as usize;
                let count = word(table, at + 8) as usize;
                for index in start..start + count {
                    output.push(word(VALUES, index * 4));
                }
                return true;
            }
        }
    }
    false
}
pub(super) fn properties(cp: u32) -> (u8, u8) {
    let mut first = 0;
    let mut last = PROPERTIES.len() / 10;
    while first < last {
        let mid = first + (last - first) / 2;
        let at = mid * 10;
        if cp < word(PROPERTIES, at) {
            last = mid;
        } else if cp > word(PROPERTIES, at + 4) {
            first = mid + 1;
        } else {
            return (PROPERTIES[at + 8], PROPERTIES[at + 9]);
        }
    }
    unreachable!("complete pinned Unicode property domain")
}
pub(super) fn composition(first_cp: u32, last_cp: u32) -> Option<u32> {
    let mut first = 0;
    let mut last = COMPOSE.len() / 12;
    while first < last {
        let mid = first + (last - first) / 2;
        let at = mid * 12;
        let pair = (word(COMPOSE, at), word(COMPOSE, at + 4));
        match (first_cp, last_cp).cmp(&pair) {
            std::cmp::Ordering::Less => last = mid,
            std::cmp::Ordering::Greater => first = mid + 1,
            std::cmp::Ordering::Equal => return Some(word(COMPOSE, at + 8)),
        }
    }
    None
}
