use super::{
    cache::Cache,
    strips::{merge_mesh_strip, MarkingSpec, MeshStripGroups},
    Color, MeshStrip, Vec3,
};
use std::sync::Arc;

type StyleKey = (usize, Vec<MarkingSpec>, Vec<f64>);
type PointRefs = Vec<(Arc<[Vec3]>, Arc<[Vec3]>)>;
type MergedValue = (PointRefs, Arc<[Vec3]>, Arc<[Vec3]>);

pub struct StyleCache {
    styled: Cache<StyleKey, (MeshStripGroups, MeshStripGroups)>,
    merged: Cache<Vec<(usize, usize)>, MergedValue>,
}
impl Default for StyleCache {
    fn default() -> Self {
        Self {
            styled: Cache::new(128),
            merged: Cache::new(128),
        }
    }
}

impl StyleCache {
    #[must_use]
    pub fn style(
        &mut self,
        groups: &MeshStripGroups,
        specs: &[MarkingSpec],
        shift: f64,
    ) -> MeshStripGroups {
        self.style_by_shift(groups, specs, &vec![shift; specs.len()])
    }

    #[must_use]
    pub fn style_by_shift(
        &mut self,
        groups: &MeshStripGroups,
        specs: &[MarkingSpec],
        shifts: &[f64],
    ) -> MeshStripGroups {
        let key = (
            Arc::as_ptr(groups).cast::<()>() as usize,
            specs.to_vec(),
            shifts.to_vec(),
        );
        if let Some((_, styled)) = self.styled.get(&key) {
            return styled;
        }
        let result: MeshStripGroups = groups
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let color = specs[index].1;
                let shift = shifts.get(index).copied().unwrap_or(0.0);
                let has_shift = shift.abs() > 0.0001;
                group
                    .iter()
                    .map(|strip| {
                        let mut strip = strip.clone();
                        if has_shift || strip.color != color {
                            strip.color = color;
                            strip.x_offset_m += shift;
                        }
                        strip
                    })
                    .collect()
            })
            .collect::<Vec<_>>()
            .into();
        self.styled
            .insert(key, (Arc::clone(groups), Arc::clone(&result)));
        result
    }

    fn merged_strip(&mut self, strips: &[MeshStrip]) -> Option<MeshStrip> {
        let first = strips.first()?;
        if strips.len() == 1 {
            return Some(first.clone());
        }
        let key = strips
            .iter()
            .map(|strip| {
                (
                    Arc::as_ptr(&strip.left).cast::<()>() as usize,
                    Arc::as_ptr(&strip.right).cast::<()>() as usize,
                )
            })
            .collect();
        if let Some((_, left, right)) = self.merged.get(&key) {
            return Some(MeshStrip {
                left,
                right,
                color: first.color,
                x_offset_m: first.x_offset_m,
            });
        }
        let merged = merge_mesh_strip(strips)?;
        let refs = strips
            .iter()
            .map(|strip| (Arc::clone(&strip.left), Arc::clone(&strip.right)))
            .collect();
        self.merged.insert(
            key,
            (refs, Arc::clone(&merged.left), Arc::clone(&merged.right)),
        );
        Some(merged)
    }

    #[must_use]
    pub fn merge_by_style(
        &mut self,
        strips: impl IntoIterator<Item = MeshStrip>,
    ) -> Vec<MeshStrip> {
        let mut groups: Vec<((Color, f64), Vec<MeshStrip>)> = Vec::new();
        for strip in strips {
            if strip.left.len().min(strip.right.len()) < 2 {
                continue;
            }
            let key = (strip.color, strip.x_offset_m);
            if let Some((_, group)) = groups.iter_mut().find(|(candidate, _)| candidate == &key) {
                group.push(strip);
            } else {
                groups.push((key, vec![strip]));
            }
        }
        let mut result = Vec::new();
        for (_, group) in groups {
            if let Some(merged) = self.merged_strip(&group) {
                result.push(merged);
            } else {
                result.extend(group);
            }
        }
        result
    }
}
