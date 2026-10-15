//! Complete-link merge order, preserving existing physical ownership preference.
use std::collections::{BTreeMap, BTreeSet};
#[derive(serde::Serialize)]
pub struct Clusters {
    pub clusters: Vec<Vec<usize>>,
    pub masks: Vec<u32>,
    pub compatible: Vec<u32>,
    pub owners: Vec<BTreeSet<u32>>,
}
pub fn merge(
    compatible: &[u32],
    pair_cost: &[f64],
    ids: &[u32],
    owner: &BTreeMap<u32, u32>,
    max_members: usize,
) -> Clusters {
    let n = ids.len();
    let mut out = Clusters {
        clusters: (0..n).map(|i| vec![i]).collect(),
        masks: (0..n).map(|i| 1 << i).collect(),
        compatible: compatible
            .iter()
            .enumerate()
            .map(|(i, &c)| c | (1 << i))
            .collect(),
        owners: ids
            .iter()
            .map(|id| owner.get(id).copied().into_iter().collect())
            .collect(),
    };
    let mut member_cluster: Vec<_> = (0..n).collect();
    loop {
        let mut best: Option<(bool, f64, usize, usize)> = None;
        for (i, a) in out.clusters.iter().enumerate() {
            let mut reach = out.compatible[i] & !out.masks[i];
            if reach == 0 {
                continue;
            }
            let limit = max_members - a.len();
            let mut seen = 0u32;
            while reach != 0 {
                let bit = 1u32 << reach.trailing_zeros();
                reach ^= bit;
                let j = member_cluster[bit.trailing_zeros() as usize];
                let mark = 1u32 << j;
                if j <= i || seen & mark != 0 {
                    continue;
                }
                seen |= mark;
                let b = &out.clusters[j];
                if b.len() > limit || out.compatible[i] & out.masks[j] != out.masks[j] {
                    continue;
                }
                let mut cost = f64::NEG_INFINITY;
                for &x in a {
                    for &y in b {
                        let v = pair_cost[x * n + y];
                        if v > cost {
                            cost = v;
                        }
                    }
                }
                let choice = (out.owners[i].is_disjoint(&out.owners[j]), cost, i, j);
                if best.is_none_or(|previous| choice < previous) {
                    best = Some(choice);
                }
            }
        }
        let Some((_, _, i, j)) = best else {
            break;
        };
        let members = out.clusters.remove(j);
        out.clusters[i].extend(members);
        let mask = out.masks.remove(j);
        out.masks[i] |= mask;
        let compatible = out.compatible.remove(j);
        out.compatible[i] &= compatible;
        let owners = out.owners.remove(j);
        out.owners[i].extend(owners);
        for (index, cluster) in out.clusters.iter().enumerate().skip(j) {
            for &member in cluster {
                member_cluster[member] = index;
            }
        }
        for &member in &out.clusters[i] {
            member_cluster[member] = i;
        }
    }
    out
}
