use super::gallop;
pub(super) fn merge(values: &mut [f64], split: usize, threshold: &mut usize) {
    if split <= values.len() - split {
        low(values, split, threshold);
    } else {
        high(values, split, threshold);
    }
}
fn low(values: &mut [f64], split: usize, threshold: &mut usize) {
    let a = values[..split].to_vec();
    let (mut ai, mut bi, mut dest) = (0, split, 0);
    values[dest] = values[bi];
    dest += 1;
    bi += 1;
    let mut minimum = *threshold;
    'merge: while ai < a.len() - 1 && bi < values.len() {
        let (mut acount, mut bcount) = (0, 0);
        loop {
            if values[bi] < a[ai] {
                values[dest] = values[bi];
                bi += 1;
                dest += 1;
                bcount += 1;
                acount = 0;
                if bi == values.len() {
                    break 'merge;
                }
                if bcount >= minimum {
                    break;
                }
            } else {
                values[dest] = a[ai];
                ai += 1;
                dest += 1;
                acount += 1;
                bcount = 0;
                if ai == a.len() - 1 {
                    break 'merge;
                }
                if acount >= minimum {
                    break;
                }
            }
        }
        minimum += 1;
        loop {
            minimum -= usize::from(minimum > 1);
            *threshold = minimum;
            acount = gallop::right(values[bi], &a[ai..], 0);
            values[dest..dest + acount].copy_from_slice(&a[ai..ai + acount]);
            dest += acount;
            ai += acount;
            if a.len() - ai <= 1 {
                break 'merge;
            }
            values[dest] = values[bi];
            dest += 1;
            bi += 1;
            if bi == values.len() {
                break 'merge;
            }
            bcount = gallop::left(a[ai], &values[bi..], 0);
            values.copy_within(bi..bi + bcount, dest);
            dest += bcount;
            bi += bcount;
            if bi == values.len() {
                break 'merge;
            }
            values[dest] = a[ai];
            dest += 1;
            ai += 1;
            if ai == a.len() - 1 {
                break 'merge;
            }
            if acount < 7 && bcount < 7 {
                break;
            }
        }
        minimum += 1;
        *threshold = minimum;
    }
    if a.len() - ai == 1 && bi < values.len() {
        let remaining = values.len() - bi;
        values.copy_within(bi.., dest);
        values[dest + remaining] = a[ai];
    } else {
        values[dest..dest + a.len() - ai].copy_from_slice(&a[ai..]);
    }
}
fn high(values: &mut [f64], split: usize, threshold: &mut usize) {
    let b = values[split..].to_vec();
    let (mut na, mut nb, mut dest) = (split, b.len(), values.len());
    na -= 1;
    dest -= 1;
    values[dest] = values[na];
    let mut minimum = *threshold;
    'merge: while na > 0 && nb > 1 {
        let (mut acount, mut bcount) = (0, 0);
        loop {
            dest -= 1;
            if b[nb - 1] < values[na - 1] {
                na -= 1;
                values[dest] = values[na];
                acount += 1;
                bcount = 0;
                if na == 0 {
                    break 'merge;
                }
                if acount >= minimum {
                    break;
                }
            } else {
                nb -= 1;
                values[dest] = b[nb];
                bcount += 1;
                acount = 0;
                if nb == 1 {
                    break 'merge;
                }
                if bcount >= minimum {
                    break;
                }
            }
        }
        minimum += 1;
        loop {
            minimum -= usize::from(minimum > 1);
            *threshold = minimum;
            acount = na - gallop::right(b[nb - 1], &values[..na], na - 1);
            values.copy_within(na - acount..na, dest - acount);
            na -= acount;
            dest -= acount;
            if na == 0 {
                break 'merge;
            }
            dest -= 1;
            nb -= 1;
            values[dest] = b[nb];
            if nb == 1 {
                break 'merge;
            }
            bcount = nb - gallop::left(values[na - 1], &b[..nb], nb - 1);
            values[dest - bcount..dest].copy_from_slice(&b[nb - bcount..nb]);
            nb -= bcount;
            dest -= bcount;
            if nb <= 1 {
                break 'merge;
            }
            dest -= 1;
            na -= 1;
            values[dest] = values[na];
            if na == 0 {
                break 'merge;
            }
            if acount < 7 && bcount < 7 {
                break;
            }
        }
        minimum += 1;
        *threshold = minimum;
    }
    if nb == 1 && na > 0 {
        values.copy_within(..na, dest - na);
        values[dest - na - 1] = b[0];
    } else {
        values[dest - nb..dest].copy_from_slice(&b[..nb]);
    }
}
