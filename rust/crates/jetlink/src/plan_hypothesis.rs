//! The original parser's two NumPy 2.5.3 descending argsorts (including ties).
//! Table enumerates five rank values in [0,4], source lockfile numpy=2.5.3.
pub fn winner(raw: [f32; 5]) -> usize {
    let mut weights = raw;
    openpilot_modeld::parse::softmax(&mut weights);
    let mut key = 0usize;
    for value in weights {
        let rank = weights.iter().filter(|&&other| other < value).count();
        key = key * 5 + rank;
    }
    usize::from(include_bytes!("../data/plan_winner_numpy253.bin")[key])
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_source_rank_patterns_keep_numpy_tie_selection() {
        let source = include_bytes!("../data/plan_winner_numpy253.bin");
        for (index, &expected) in source.iter().enumerate() {
            let mut key = index;
            let mut scores = [0.; 5];
            for i in (0..5).rev() {
                scores[i] = (key % 5) as f32;
                key /= 5;
            }
            assert_eq!(super::winner(scores), usize::from(expected), "{scores:?}");
        }
    }
}
