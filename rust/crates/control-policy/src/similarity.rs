use std::collections::HashMap;

pub fn ratio(left: &str, right: &str) -> f64 {
    let a: Vec<char> = left.chars().collect();
    let b: Vec<char> = right.chars().collect();
    if a.len() + b.len() == 0 {
        return 1.;
    }
    let mut positions: HashMap<char, Vec<usize>> = HashMap::new();
    for (j, c) in b.iter().enumerate() {
        positions.entry(*c).or_default().push(j);
    }
    if b.len() >= 200 {
        positions.retain(|_, indices| indices.len() <= b.len() / 100 + 1);
    }
    let mut queue = vec![(0, a.len(), 0, b.len())];
    let mut matched = 0;
    while let Some((a_low, a_high, b_low, b_high)) = queue.pop() {
        let (mut best_a, mut best_b, mut size) = (a_low, b_low, 0);
        let mut lengths: HashMap<usize, usize> = HashMap::new();
        for (i, c) in a.iter().enumerate().take(a_high).skip(a_low) {
            let mut next = HashMap::new();
            if let Some(indices) = positions.get(c) {
                for &j in indices {
                    if j < b_low {
                        continue;
                    }
                    if j >= b_high {
                        break;
                    }
                    let n = j
                        .checked_sub(1)
                        .and_then(|j| lengths.get(&j))
                        .copied()
                        .unwrap_or(0)
                        + 1;
                    next.insert(j, n);
                    if n > size {
                        best_a = i + 1 - n;
                        best_b = j + 1 - n;
                        size = n;
                    }
                }
            }
            lengths = next;
        }
        while best_a > a_low && best_b > b_low && a[best_a - 1] == b[best_b - 1] {
            best_a -= 1;
            best_b -= 1;
            size += 1;
        }
        while best_a + size < a_high
            && best_b + size < b_high
            && a[best_a + size] == b[best_b + size]
        {
            size += 1;
        }
        if size > 0 {
            matched += size;
            if a_low < best_a && b_low < best_b {
                queue.push((a_low, best_a, b_low, best_b));
            }
            if best_a + size < a_high && best_b + size < b_high {
                queue.push((best_a + size, a_high, best_b + size, b_high));
            }
        }
    }
    2. * matched as f64 / (a.len() + b.len()) as f64
}

pub fn select<'a>(files: &'a [String], fingerprint: &str, firmware: &str) -> Option<&'a str> {
    let full = if firmware.len() > 3 {
        format!("{fingerprint} {}", firmware.replace('\\', ""))
    } else {
        fingerprint.into()
    };
    for query in [&full, fingerprint] {
        let mut best: Option<(&str, f64)> = None;
        for file in files {
            if !file.ends_with(".json") {
                continue;
            }
            let model = file.replace(".json", "");
            let score = ratio(&model, query);
            if best.is_none_or(|(_, old)| score > old) {
                best = Some((file, score));
            }
        }
        if let Some((file, score)) = best {
            if file.contains(fingerprint) && !(0. ..0.9).contains(&score) {
                return Some(file);
            }
        }
    }
    None
}
