//! First-frame FLI/FLC index decoding; palette metadata does not alter indices.
use super::{array, check_dimensions, le16, le32, range, zeroed, Array, Result};
fn byte(b: &[u8], at: &mut usize) -> Result<usize> {
    let v = range(b, *at, 1)?[0] as usize;
    *at += 1;
    Ok(v)
}
fn word(b: &[u8], at: &mut usize) -> Result<usize> {
    let v = le16(b, *at)?;
    *at += 2;
    Ok(v)
}
fn write(dst: &mut [i64], w: usize, h: usize, x: usize, y: usize, data: &[u8]) -> Result<()> {
    if y >= h || x > w || data.len() > w - x {
        return Err("FLI packet overrun".into());
    }
    for (i, v) in data.iter().enumerate() {
        dst[y * w + x + i] = i64::from(*v);
    }
    Ok(())
}
pub(super) fn fli(b: &[u8]) -> Result<Array> {
    range(b, 0, 128)?;
    if ![0, 3].contains(&le16(b, 14)?)
        || range(b, 20, 2)? != [0; 2]
        || range(b, 42, 38)? != [0; 38]
        || range(b, 88, 40)? != [0; 40]
    {
        return Err("invalid FLI header".into());
    }
    let (w, h) = (le16(b, 8)?, le16(b, 10)?);
    check_dimensions(w, h)?;
    let frame = &b[128..];
    let size = le32(frame, 0)?;
    if frame.len() + frame.len() % 2 < size || le16(frame, 4)? != 0xf1fa {
        return Err("invalid FLI frame".into());
    }
    let mut values = zeroed(w * h)?;
    let mut at = 16;
    for _ in 0..le16(frame, 6)? {
        range(frame, at, 10)?;
        let advance = le32(frame, at)?;
        let kind = le16(frame, at + 4)?;
        let mut p = at + 6;
        match kind {
            4 | 11 | 18 => {}
            13 => values.fill(0),
            16 => {
                for (out, v) in values.iter_mut().zip(range(frame, p, w * h)?) {
                    *out = i64::from(*v);
                }
            }
            15 => {
                for y in 0..h {
                    p += 1;
                    let mut x = 0;
                    while x < w {
                        let count = byte(frame, &mut p)?;
                        let data = if count >= 128 {
                            let n = 256 - count;
                            let raw = range(frame, p, n)?.to_vec();
                            p += n;
                            raw
                        } else {
                            let v = byte(frame, &mut p)? as u8;
                            vec![v; count]
                        };
                        write(&mut values, w, h, x, y, &data)?;
                        x += data.len();
                    }
                }
            }
            12 => {
                let start = word(frame, &mut p)?;
                let lines = word(frame, &mut p)?;
                for y in start..start + lines {
                    let packets = byte(frame, &mut p)?;
                    let mut x = 0;
                    for _ in 0..packets {
                        x += byte(frame, &mut p)?;
                        let n = byte(frame, &mut p)?;
                        let data = if n >= 128 {
                            let v = byte(frame, &mut p)? as u8;
                            vec![v; 256 - n]
                        } else {
                            let raw = range(frame, p, n)?.to_vec();
                            p += n;
                            raw
                        };
                        write(&mut values, w, h, x, y, &data)?;
                        x += data.len();
                    }
                }
            }
            7 => {
                let lines = word(frame, &mut p)?;
                let mut y = 0;
                for _ in 0..lines {
                    let mut packets = word(frame, &mut p)?;
                    while packets & 0x8000 != 0 {
                        if packets & 0x4000 != 0 {
                            y += 65536 - packets;
                            if y >= h {
                                return Err("FLI skipped row overrun".into());
                            }
                        } else {
                            write(&mut values, w, h, w - 1, y, &[packets as u8])?;
                        }
                        packets = word(frame, &mut p)?;
                    }
                    let mut x = 0;
                    for _ in 0..packets {
                        x += byte(frame, &mut p)?;
                        let n = byte(frame, &mut p)?;
                        let data = if n >= 128 {
                            let v = range(frame, p, 2)?;
                            p += 2;
                            v.repeat(256 - n)
                        } else {
                            let v = range(frame, p, n * 2)?.to_vec();
                            p += n * 2;
                            v
                        };
                        write(&mut values, w, h, x, y, &data)?;
                        x += data.len();
                    }
                    y += 1;
                }
            }
            _ => return Err("unsupported FLI chunk".into()),
        }
        if advance == 0 || advance > frame.len() - at {
            return Err("FLI chunk advance".into());
        }
        at += advance;
    }
    Ok(array("P", w, h, 1, values))
}
