//! Float-specialized CPython v3.12.14 list sort (PSF license; see NOTICE).
mod gallop;
mod merge;
#[derive(Clone, Copy)]
struct Run {
    start: usize,
    len: usize,
    power: usize,
}
pub(crate) fn sort(values: &mut [f64]) {
    if values.len() < 2 {
        return;
    }
    let mut minimum = values.len();
    let mut remainder = 0;
    while minimum >= 64 {
        remainder |= minimum & 1;
        minimum >>= 1;
    }
    minimum += remainder;
    let mut runs: Vec<Run> = Vec::new();
    let mut start = 0;
    let mut gallop = 7;
    while start < values.len() {
        let rest = &mut values[start..];
        let mut len = 1;
        if rest.len() > 1 {
            let descending = rest[1] < rest[0];
            len = 2;
            while len < rest.len() && (rest[len] < rest[len - 1]) == descending {
                len += 1;
            }
            if descending {
                rest[..len].reverse();
            }
        }
        let force = minimum.min(rest.len());
        if len < force {
            for position in len..force {
                let pivot = rest[position];
                let (mut left, mut right) = (0, position);
                while left < right {
                    let middle = left + (right - left) / 2;
                    if pivot < rest[middle] {
                        right = middle;
                    } else {
                        left = middle + 1;
                    }
                }
                rest.copy_within(left..position, left + 1);
                rest[left] = pivot;
            }
            len = force;
        }
        if let Some(previous) = runs.last().copied() {
            let power = power(previous, len, values.len());
            while runs.len() > 1 && runs[runs.len() - 2].power > power {
                let index = runs.len() - 2;
                merge_at(values, &mut runs, index, &mut gallop);
            }
            if let Some(previous) = runs.last_mut() {
                previous.power = power;
            }
        }
        runs.push(Run {
            start,
            len,
            power: 0,
        });
        start += len;
    }
    while runs.len() > 1 {
        let mut index = runs.len() - 2;
        if index > 0 && runs[index - 1].len < runs[index + 1].len {
            index -= 1;
        }
        merge_at(values, &mut runs, index, &mut gallop);
    }
}
fn power(run: Run, next: usize, total: usize) -> usize {
    let mut a = 2 * run.start + run.len;
    let mut b = a + run.len + next;
    let mut power = 0;
    loop {
        power += 1;
        if a >= total {
            a -= total;
            b -= total;
        } else if b >= total {
            break;
        }
        a *= 2;
        b *= 2;
    }
    power
}
fn merge_at(values: &mut [f64], runs: &mut Vec<Run>, index: usize, threshold: &mut usize) {
    let a = runs[index];
    let b = runs.remove(index + 1);
    runs[index].len += b.len;
    let skipped = gallop::right(values[b.start], &values[a.start..b.start], 0);
    let start = a.start + skipped;
    if start == b.start {
        return;
    }
    let kept = gallop::left(
        values[b.start - 1],
        &values[b.start..b.start + b.len],
        b.len - 1,
    );
    if kept == 0 {
        return;
    }
    merge::merge(
        &mut values[start..b.start + kept],
        b.start - start,
        threshold,
    );
}
