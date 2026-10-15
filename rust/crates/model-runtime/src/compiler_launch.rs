//! Offline A630 launch selection, derived from model_export/offline_schedule.py.
//! Explicit launches retain their original dimension types and values.

use unicode_general_category::{get_general_category, GeneralCategory};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dimension {
    Integer(i128),
    Float(f64),
    Boolean(bool),
}

pub fn launch_dimensions(
    global: &[Dimension],
    local: Option<&[Dimension]>,
    source: &str,
    binary: &[u8],
) -> Result<(Vec<Dimension>, Vec<Dimension>), String> {
    if let Some(local) = local {
        return Ok((global.to_vec(), local.to_vec()));
    }
    if global.len() != 3
        || global.iter().any(|value| {
            !matches!(value, Dimension::Integer(size) if (1..=u32::MAX as i128).contains(size))
        })
    {
        return Err("unsupported offline QCOM launch dimensions".into());
    }
    // Match the original word-boundary search, including tokens in comments:
    // regrouping must not change any source-visible group or local IDs.
    if source
        .split(|ch: char| {
            ch != '_'
                && !matches!(
                    get_general_category(ch),
                    GeneralCategory::UppercaseLetter
                        | GeneralCategory::LowercaseLetter
                        | GeneralCategory::TitlecaseLetter
                        | GeneralCategory::ModifierLetter
                        | GeneralCategory::OtherLetter
                        | GeneralCategory::DecimalNumber
                        | GeneralCategory::LetterNumber
                        | GeneralCategory::OtherNumber
                )
        })
        .any(|token| {
            matches!(
                token,
                "get_local_id"
                    | "get_group_id"
                    | "get_local_size"
                    | "get_num_groups"
                    | "barrier"
                    | "work_group_barrier"
                    | "__local"
            )
        })
    {
        return Ok((global.to_vec(), vec![Dimension::Integer(1); 3]));
    }
    let read = |offset: usize| -> Result<u32, String> {
        let end = offset
            .checked_add(4)
            .ok_or("offline QCOM binary lacks register metadata")?;
        let bytes = binary
            .get(offset..end)
            .ok_or("offline QCOM binary lacks register metadata")?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
    };
    let registers = read(0x34)? as usize;
    let offset = registers
        .checked_add(0x14)
        .ok_or("offline QCOM binary lacks register metadata")?;
    let full = read(offset)?;
    let half = read(
        offset
            .checked_add(4)
            .ok_or("offline QCOM binary lacks register metadata")?,
    )?;
    if registers < 0x38 || full > 63 || half > 63 {
        return Err("unsupported offline QCOM register metadata".into());
    }
    let max_threads = 1024.min((384 * 32 / ((full + half.div_ceil(2)).max(1) * 128)) * 128);
    let mut remaining = 64.min(max_threads) as i128;
    let mut groups = Vec::with_capacity(3);
    let mut locals = Vec::with_capacity(3);
    for value in global {
        let Dimension::Integer(size) = value else {
            unreachable!("integer dimensions admitted above")
        };
        let mut group = 1;
        while group * 2 <= remaining && size % (group * 2) == 0 {
            group *= 2;
        }
        groups.push(Dimension::Integer(size / group));
        locals.push(Dimension::Integer(group));
        remaining /= group;
    }
    Ok((groups, locals))
}
