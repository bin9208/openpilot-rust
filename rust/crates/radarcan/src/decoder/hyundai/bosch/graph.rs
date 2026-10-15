//! Source-compatible optional raw association and uniqueness certificates.
use super::assignment;
use std::collections::{BTreeMap, BTreeSet};
pub type Edges = Vec<Vec<(usize, f64)>>;
type Matching = BTreeMap<usize, usize>;
#[derive(Debug, serde::Serialize)]
pub struct Result {
    pub assignment: Matching,
    pub components: usize,
    pub largest: usize,
    pub certified: usize,
    pub fallback: bool,
    pub ambiguous: BTreeSet<usize>,
}
fn cycle(adjacency: &[Vec<usize>], real: &[bool]) -> bool {
    let size = adjacency.len();
    let mut index = vec![0; size];
    let mut low = vec![0; size];
    let mut seen = vec![false; size];
    let mut on_stack = vec![false; size];
    let mut stack = Vec::new();
    let mut counter = 1;
    for root in 0..size {
        if seen[root] {
            continue;
        }
        let mut work = vec![(root, 0)];
        while let Some(&(vertex, mut position)) = work.last() {
            if position == 0 {
                seen[vertex] = true;
                index[vertex] = counter;
                low[vertex] = counter;
                counter += 1;
                stack.push(vertex);
                on_stack[vertex] = true;
            }
            let mut descended = false;
            while position < adjacency[vertex].len() {
                let other = adjacency[vertex][position];
                position += 1;
                if !seen[other] {
                    work.last_mut().unwrap().1 = position;
                    work.push((other, 0));
                    descended = true;
                    break;
                }
                if on_stack[other] && index[other] < low[vertex] {
                    low[vertex] = index[other];
                }
            }
            if descended {
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                if low[vertex] < low[parent] {
                    low[parent] = low[vertex];
                }
            }
            if low[vertex] == index[vertex] {
                let mut count = 0;
                let mut has_real = false;
                loop {
                    let other = stack.pop().unwrap();
                    on_stack[other] = false;
                    count += 1;
                    has_real |= real[other];
                    if other == vertex {
                        break;
                    }
                }
                if count > 1 && has_real {
                    return true;
                }
            }
        }
    }
    false
}
fn rectangular(
    rows: &[usize],
    columns: &[usize],
    edges: &Edges,
    unmatched: f64,
    margin: f64,
) -> Option<Matching> {
    let (a, b) = (rows.len(), columns.len());
    let tolerance = margin.max(1e-12);
    let threshold = 4. * tolerance * (a + b + 1) as f64;
    if !threshold.is_finite() {
        return None;
    }
    let at: BTreeMap<_, _> = columns.iter().enumerate().map(|(j, &v)| (v, j)).collect();
    let mut costs = vec![vec![f64::INFINITY; a + b]; a];
    for (i, vertex) in rows.iter().enumerate() {
        costs[i][b + i] = 2. * unmatched;
        for &(other, cost) in &edges[*vertex] {
            costs[i][*at.get(&other)?] = cost;
        }
    }
    let solved = assignment::solve(&costs, a + b).ok()?;
    let (u, v) = (&solved.row_potential, &solved.column_potential);
    if solved.rows.len() != a || u.iter().chain(v).any(|x| !x.is_finite()) {
        return None;
    }
    let mut occupied = vec![false; a + b];
    for &j in &solved.columns {
        occupied[j] = true;
    }
    let reduced: Vec<Vec<_>> = costs
        .iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .map(|(j, c)| (*c - u[i]) - v[j])
                .collect()
        })
        .collect();
    if reduced.iter().flatten().any(|&x| x < -tolerance)
        || solved
            .rows
            .iter()
            .zip(&solved.columns)
            .any(|(&i, &j)| reduced[i][j].abs() > tolerance)
        || v.iter()
            .enumerate()
            .any(|(j, &x)| x > tolerance || (!occupied[j] && x.abs() > tolerance))
    {
        return None;
    }
    let reservoir = 2 * a + b;
    let mut adjacency = vec![vec![]; reservoir + 1];
    let mut matching = Matching::new();
    for (&i, &matched) in solved.rows.iter().zip(&solved.columns) {
        adjacency[a + matched].push(i);
        for (j, &value) in reduced[i].iter().enumerate() {
            if j != matched && value <= threshold {
                adjacency[i].push(a + j);
            }
        }
        if matched < b {
            matching.insert(columns[matched], rows[i]);
        }
    }
    for j in 0..a + b {
        if !occupied[j] {
            adjacency[a + j].push(reservoir);
        } else if v[j] >= -threshold {
            adjacency[reservoir].push(a + j);
        }
    }
    let mut real = vec![false; reservoir + 1];
    real[..a].fill(true);
    (!cycle(&adjacency, &real)).then_some(matching)
}
fn component(
    rows: &[usize],
    columns: &[usize],
    edges: &Edges,
    unmatched: f64,
    margin: Option<f64>,
) -> Option<(Matching, bool)> {
    if let Some(margin) = margin {
        if let Some(matching) = rectangular(rows, columns, edges, unmatched, margin) {
            return Some((matching, true));
        }
    }
    let (a, b) = (rows.len(), columns.len());
    let size = a + b;
    let at: BTreeMap<_, _> = columns.iter().enumerate().map(|(j, &v)| (v, j)).collect();
    let mut costs = vec![vec![f64::INFINITY; size]; size];
    for (i, vertex) in rows.iter().enumerate() {
        costs[i][b + i] = unmatched;
        for &(other, cost) in &edges[*vertex] {
            costs[i][*at.get(&other)?] = cost;
        }
    }
    for j in 0..b {
        costs[a + j][j] = unmatched;
        costs[a + j][b..].fill(0.);
    }
    let solved = assignment::solve(&costs, size).ok()?;
    if solved.rows.len() != size {
        return None;
    }
    let matching: Matching = solved
        .rows
        .iter()
        .zip(&solved.columns)
        .filter_map(|(&i, &j)| {
            if i < a && j < b {
                Some((columns[j], rows[i]))
            } else {
                None
            }
        })
        .collect();
    let Some(margin) = margin else {
        return Some((matching, false));
    };
    let mut adjacency = vec![vec![]; 2 * size];
    for (&i, &matched) in solved.rows.iter().zip(&solved.columns) {
        adjacency[size + matched].push(i);
        for (j, &cost) in costs[i].iter().enumerate() {
            if j != matched
                && (cost - solved.row_potential[i]) - solved.column_potential[j]
                    <= margin.max(1e-12)
            {
                adjacency[i].push(size + j);
            }
        }
    }
    let mut real = vec![false; 2 * size];
    real[..a].fill(true);
    real[size..size + b].fill(true);
    Some((matching, !cycle(&adjacency, &real)))
}
#[derive(Clone)]
struct Optimum {
    best: f64,
    second: f64,
    matching: Vec<(usize, usize)>,
}
type Options = Vec<Vec<(u64, f64, (usize, usize))>>;
fn subset(
    index: usize,
    occupied: u64,
    options: &Options,
    memo: &mut BTreeMap<(usize, u64), Optimum>,
) -> Optimum {
    if index == options.len() {
        return Optimum {
            best: 0.,
            second: f64::NEG_INFINITY,
            matching: vec![],
        };
    }
    if let Some(cached) = memo.get(&(index, occupied)) {
        return cached.clone();
    }
    let mut result = subset(index + 1, occupied, options, memo);
    for &(bit, saving, pair) in &options[index] {
        if occupied & bit != 0 {
            continue;
        }
        let child = subset(index + 1, occupied | bit, options, memo);
        let candidate = saving + child.best;
        if candidate > result.best {
            result.second = result.best;
            result.best = candidate;
            result.matching = vec![pair];
            result.matching.extend(child.matching);
        } else if candidate > result.second {
            result.second = candidate;
        }
        let runner_up = saving + child.second;
        if runner_up > result.second {
            result.second = runner_up;
        }
    }
    memo.insert((index, occupied), result.clone());
    result
}
fn unique(
    rows: &[usize],
    columns: &[usize],
    row_edges: &Edges,
    column_edges: &Edges,
    unmatched: f64,
) -> Option<Matching> {
    let reward = 2. * unmatched;
    let margin = 1e-12 * reward.abs().max(1.) * (rows.len() + columns.len() + 1) as f64;
    if !margin.is_finite() {
        return None;
    }
    if rows.len() == 1 && columns.len() == 1 {
        let saving = reward - row_edges[rows[0]][0].1;
        return if saving > margin {
            Some([(columns[0], rows[0])].into())
        } else if -saving > margin {
            Some(Matching::new())
        } else {
            None
        };
    }
    for (vertices, edges, transposed) in [(rows, row_edges, false), (columns, column_edges, true)] {
        let mut matching = Matching::new();
        let mut used = BTreeSet::new();
        let mut valid = true;
        for vertex in vertices {
            let (mut best, mut second, mut partner) = (0., f64::NEG_INFINITY, None);
            for &(other, cost) in &edges[*vertex] {
                let saving = reward - cost;
                if saving > best {
                    second = best;
                    best = saving;
                    partner = Some(other);
                } else if saving > second {
                    second = saving;
                }
            }
            if best - second <= margin || partner.is_some_and(|p| used.contains(&p)) {
                valid = false;
                break;
            }
            if let Some(partner) = partner {
                used.insert(partner);
                if transposed {
                    matching.insert(*vertex, partner);
                } else {
                    matching.insert(partner, *vertex);
                }
            }
        }
        if valid {
            return Some(matching);
        }
    }
    if rows.len().min(columns.len()) > 6 || rows.len().max(columns.len()) > 12 {
        return component(rows, columns, row_edges, unmatched, Some(margin))
            .and_then(|(matching, unique)| unique.then_some(matching));
    }
    let transposed = rows.len() < columns.len();
    let (small, large, edges) = if transposed {
        (rows, columns, column_edges)
    } else {
        (columns, rows, row_edges)
    };
    let bits: BTreeMap<_, _> = small
        .iter()
        .enumerate()
        .map(|(i, &v)| (v, 1u64 << i))
        .collect();
    let options = large
        .iter()
        .map(|&vertex| {
            edges[vertex]
                .iter()
                .map(|&(other, cost)| {
                    (
                        bits[&other],
                        reward - cost,
                        if transposed {
                            (vertex, other)
                        } else {
                            (other, vertex)
                        },
                    )
                })
                .collect()
        })
        .collect();
    let result = subset(0, 0, &options, &mut BTreeMap::new());
    (result.best - result.second > margin).then(|| result.matching.into_iter().collect())
}
pub fn solve(
    n: usize,
    m: usize,
    row_edges: &Edges,
    column_edges: &Edges,
    unmatched: f64,
) -> std::result::Result<Result, &'static str> {
    let mut components = Vec::new();
    let mut visited_rows = BTreeSet::new();
    let mut visited_columns = BTreeSet::new();
    for first in 0..n {
        let edges = &row_edges[first];
        if edges.is_empty() || visited_rows.contains(&first) {
            continue;
        }
        if edges.len() == 1 && column_edges[edges[0].0].len() == 1 {
            visited_rows.insert(first);
            visited_columns.insert(edges[0].0);
            components.push((vec![first], vec![edges[0].0]));
            continue;
        }
        let (mut rows, mut columns, mut pending) = (vec![], vec![], vec![(false, first)]);
        while let Some((column, vertex)) = pending.pop() {
            if !column {
                if !visited_rows.insert(vertex) {
                    continue;
                }
                rows.push(vertex);
                pending.extend(
                    row_edges[vertex]
                        .iter()
                        .filter(|(c, _)| !visited_columns.contains(c))
                        .map(|&(c, _)| (true, c)),
                );
            } else {
                if !visited_columns.insert(vertex) {
                    continue;
                }
                columns.push(vertex);
                pending.extend(
                    column_edges[vertex]
                        .iter()
                        .filter(|(r, _)| !visited_rows.contains(r))
                        .map(|&(r, _)| (false, r)),
                );
            }
        }
        components.push((rows, columns));
    }
    let mut output = Result {
        assignment: Matching::new(),
        components: components.len(),
        largest: components
            .iter()
            .map(|(r, c)| r.len() + c.len())
            .max()
            .unwrap_or(0),
        certified: 0,
        fallback: false,
        ambiguous: BTreeSet::new(),
    };
    for (mut rows, mut columns) in components {
        let mut result = unique(&rows, &columns, row_edges, column_edges, unmatched);
        if result.is_some() {
            output.certified += 1;
        } else {
            rows.sort_unstable();
            columns.sort_unstable();
            if let Some((matching, unique)) = component(&rows, &columns, row_edges, unmatched, None)
            {
                result = Some(matching);
                if !unique {
                    output.ambiguous.extend(&columns);
                }
            }
        }
        if let Some(result) = result {
            output.assignment.extend(result);
        } else {
            let mut costs = vec![vec![f64::INFINITY; n + m]; n + m];
            for (row, edges) in row_edges.iter().enumerate() {
                costs[row][m + row] = unmatched;
                for &(column, cost) in edges {
                    costs[row][column] = cost;
                }
            }
            for column in 0..m {
                costs[n + column][column] = unmatched;
                costs[n + column][m..].fill(0.);
            }
            let solved = assignment::solve(&costs, n + m)?;
            output.assignment = solved
                .rows
                .iter()
                .zip(&solved.columns)
                .filter_map(|(&r, &c)| (r < n && c < m).then_some((c, r)))
                .collect();
            output.certified = 0;
            output.fallback = true;
            output.ambiguous.clear();
            return Ok(output);
        }
    }
    Ok(output)
}

fn physical_component(
    rows: &[usize],
    columns: &[usize],
    row_edges: &Edges,
    column_edges: &Edges,
    margin: f64,
) -> Option<Matching> {
    if rows.len() == 1 || columns.len() == 1 {
        let transposed = rows.len() != 1;
        let (vertex, edges) = if transposed {
            (columns[0], column_edges)
        } else {
            (rows[0], row_edges)
        };
        let (mut best, mut second, mut partner) = (0., 0., None);
        for &(other, score) in &edges[vertex] {
            if score > best {
                second = best;
                best = score;
                partner = Some(other);
            } else if score > second {
                second = score;
            }
        }
        return (best - second > margin).then(|| {
            let other = partner.expect("positive uniquely best score");
            if transposed {
                [(other, vertex)].into()
            } else {
                [(vertex, other)].into()
            }
        });
    }
    if rows.len() > 8 || columns.len() > 12 {
        return None;
    }
    let bits: BTreeMap<_, _> = columns
        .iter()
        .enumerate()
        .map(|(i, &column)| (column, 1u64 << i))
        .collect();
    let options = rows
        .iter()
        .map(|&row| {
            row_edges[row]
                .iter()
                .map(|&(column, score)| (bits[&column], score, (row, column)))
                .collect()
        })
        .collect();
    let result = subset(0, 0, &options, &mut BTreeMap::new());
    (result.best - result.second > margin).then(|| result.matching.into_iter().collect())
}

/// Unique physical score matching; `None` retains the source whole-scan tie path.
pub fn physical(row_edges: &Edges, column_edges: &Edges) -> Option<Matching> {
    let mut assignment = Matching::new();
    let (mut seen_rows, mut seen_columns) = (BTreeSet::new(), BTreeSet::new());
    for (first, edges) in row_edges.iter().enumerate() {
        if edges.is_empty() || seen_rows.contains(&first) {
            continue;
        }
        let (mut rows, mut columns, mut pending, mut peak) =
            (vec![], vec![], vec![(false, first)], 0.);
        while let Some((column, vertex)) = pending.pop() {
            if !column {
                if !seen_rows.insert(vertex) {
                    continue;
                }
                rows.push(vertex);
                for &(column, score) in &row_edges[vertex] {
                    if score > peak {
                        peak = score;
                    }
                    if !seen_columns.contains(&column) {
                        pending.push((true, column));
                    }
                }
            } else {
                if !seen_columns.insert(vertex) {
                    continue;
                }
                columns.push(vertex);
                pending.extend(
                    column_edges[vertex]
                        .iter()
                        .filter(|(row, _)| !seen_rows.contains(row))
                        .map(|&(row, _)| (false, row)),
                );
            }
        }
        let margin = 1e-12 * peak * (rows.len() + columns.len() + 1) as f64;
        assignment.extend(physical_component(
            &rows,
            &columns,
            row_edges,
            column_edges,
            margin,
        )?);
    }
    Some(assignment)
}
