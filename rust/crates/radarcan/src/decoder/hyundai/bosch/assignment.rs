//! Native scalar ordering of the pinned NumPy Jonker–Volgenant fallback.
#[derive(serde::Serialize)]
pub struct Assignment {
    pub rows: Vec<usize>,
    pub columns: Vec<usize>,
    pub row_potential: Vec<f64>,
    pub column_potential: Vec<f64>,
}
pub fn solve(cost: &[Vec<f64>], column_count: usize) -> Result<Assignment, &'static str> {
    if cost.iter().any(|row| row.len() != column_count) {
        return Err("expected a matrix");
    }
    if cost
        .iter()
        .flatten()
        .any(|value| value.is_nan() || *value == f64::NEG_INFINITY)
    {
        return Err("matrix contains invalid numeric entries");
    }
    let transposed = cost.len() > column_count;
    let (n, m) = if transposed {
        (column_count, cost.len())
    } else {
        (cost.len(), column_count)
    };
    if n == 0 {
        return Ok(Assignment {
            rows: Vec::new(),
            columns: Vec::new(),
            row_potential: Vec::new(),
            column_potential: Vec::new(),
        });
    }
    let mut u = vec![0.; n + 1];
    let mut v = vec![0.; m + 1];
    let mut p = vec![0; m + 1];
    let mut way = vec![0; m + 1];
    let mut minimum = vec![f64::INFINITY; m + 1];
    let mut used = vec![false; m + 1];
    let mut path_rows = vec![0; m + 1];
    for row in 1..=n {
        p[0] = row;
        minimum.fill(f64::INFINITY);
        used.fill(false);
        let mut column = 0;
        let mut depth = 0;
        loop {
            used[column] = true;
            minimum[column] = f64::INFINITY;
            let current = p[column];
            path_rows[depth] = current;
            depth += 1;
            for next in 1..=m {
                let value = if transposed {
                    cost[next - 1][current - 1]
                } else {
                    cost[current - 1][next - 1]
                };
                let reduced = (value - u[current]) - v[next];
                if reduced < minimum[next] && !used[next] {
                    minimum[next] = reduced;
                    way[next] = column;
                }
            }
            let mut next = 0;
            for index in 1..=m {
                if minimum[index] < minimum[next] {
                    next = index;
                }
            }
            let delta = minimum[next];
            if !delta.is_finite() {
                return Err("cost matrix is infeasible");
            }
            for index in &path_rows[..depth] {
                u[*index] += delta;
            }
            for index in 0..=m {
                if used[index] {
                    v[index] -= delta;
                } else {
                    minimum[index] -= delta;
                }
            }
            column = next;
            if p[column] == 0 {
                break;
            }
        }
        while column != 0 {
            let previous = way[column];
            p[column] = p[previous];
            column = previous;
        }
    }
    let mut pairs: Vec<_> = (1..=m)
        .filter(|column| p[*column] != 0)
        .map(|column| {
            if transposed {
                (column - 1, p[column] - 1)
            } else {
                (p[column] - 1, column - 1)
            }
        })
        .collect();
    pairs.sort_by_key(|pair| pair.0);
    let (rows, columns) = pairs.into_iter().unzip();
    let (row_potential, column_potential) = if transposed {
        (v[1..].to_vec(), u[1..].to_vec())
    } else {
        (u[1..].to_vec(), v[1..].to_vec())
    };
    Ok(Assignment {
        rows,
        columns,
        row_potential,
        column_potential,
    })
}
