//! qrcode 8.2 lost-point policy, including test-mode format/version bits.
use num_traits::ToPrimitive;
use qrcodegen::QrCode;
pub fn source_score(qr: &QrCode) -> u32 {
    let n = usize::try_from(qr.size()).unwrap_or(0);
    let mut grid: Vec<Vec<bool>> = (0..qr.size())
        .map(|y| (0..qr.size()).map(|x| qr.get_module(x, y)).collect())
        .collect();
    for i in 0..15 {
        let row = if i < 6 {
            i
        } else if i < 8 {
            i + 1
        } else {
            n - 15 + i
        };
        grid[row][8] = false;
        let col = if i < 8 {
            n - i - 1
        } else if i < 9 {
            15 - i
        } else {
            14 - i
        };
        grid[8][col] = false;
    }
    grid[n - 8][8] = false;
    if qr.version().value() >= 7 {
        for i in 0..18 {
            grid[i / 3][i % 3 + n - 11] = false;
            grid[i % 3 + n - 11][i / 3] = false;
        }
    }
    let mut score = 0;
    for index in 0..n {
        score += line_score(grid[index].iter().copied());
        score += line_score(grid.iter().map(|row| row[index]));
    }
    for y in 0..n - 1 {
        for x in 0..n - 1 {
            if grid[y][x] == grid[y + 1][x]
                && grid[y][x] == grid[y][x + 1]
                && grid[y][x] == grid[y + 1][x + 1]
            {
                score += 3;
            }
        }
    }
    let dark = grid
        .iter()
        .flatten()
        .filter(|v| **v)
        .count()
        .to_f64()
        .unwrap_or(0.0);
    let percent = dark / (n * n).to_f64().unwrap_or(1.0);
    score + ((percent * 100.0 - 50.0).abs() / 5.0).to_u32().unwrap_or(0) * 10
}
fn line_score(values: impl Iterator<Item = bool>) -> u32 {
    let line: Vec<_> = values.collect();
    let mut score = 0;
    let mut count = 0;
    let mut previous = false;
    for &value in &line {
        if value == previous {
            count += 1;
        } else {
            if count >= 5 {
                score += count - 2;
            }
            count = 1;
            previous = value;
        }
    }
    if count >= 5 {
        score += count - 2;
    }
    const FIRST: [bool; 11] = [
        true, false, true, true, true, false, true, false, false, false, false,
    ];
    const SECOND: [bool; 11] = [
        false, false, false, false, true, false, true, true, true, false, true,
    ];
    for part in line.windows(11) {
        if part == FIRST || part == SECOND {
            score += 40;
        }
    }
    score
}
