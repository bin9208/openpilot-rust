//! Original obfuscated defaults from server/services/popular_values.py; never emit decoded tokens.
pub(super) const DEFAULT_BASE_URL_KEY: u8 = 41;
pub(super) const DEFAULT_BASE_URL_BYTES: &[u8] = &[
    65, 93, 93, 89, 90, 19, 6, 6, 74, 89, 95, 7, 67, 70, 68, 64, 71, 66, 64, 26, 28, 29, 7, 69, 64,
    95, 76,
];
pub(super) const DEFAULT_CF_ACCESS_ID_KEY: u8 = 41;
pub(super) const DEFAULT_CF_ACCESS_ID_BYTES: &[u8] = &[
    25, 31, 76, 77, 24, 30, 75, 79, 31, 72, 75, 29, 26, 16, 17, 79, 28, 30, 28, 79, 25, 31, 72, 25,
    16, 16, 27, 31, 72, 79, 29, 75, 7, 72, 74, 74, 76, 90, 90,
];
pub(super) const DEFAULT_CF_ACCESS_SECRET_KEY: u8 = 41;
pub(super) const DEFAULT_CF_ACCESS_SECRET_BYTES: &[u8] = &[
    74, 31, 25, 29, 17, 79, 16, 74, 24, 24, 31, 29, 16, 24, 25, 76, 29, 76, 29, 26, 75, 16, 27, 29,
    24, 75, 16, 31, 27, 24, 74, 76, 76, 28, 74, 27, 30, 29, 79, 79, 75, 79, 74, 74, 75, 16, 79, 16,
    25, 72, 30, 24, 28, 29, 16, 28, 79, 16, 75, 16, 31, 75, 76, 31,
];

pub(super) fn decode(bytes: &[u8], key: u8) -> String {
    bytes.iter().map(|byte| char::from(byte ^ key)).collect()
}
