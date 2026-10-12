use super::super::{graphics::scalar, tokens::TRIANGLE_STRIP_POINT_CACHE_LIMIT, Result};
use crate::scene::{MeshStrip, Vec3};
use std::{collections::VecDeque, sync::Arc};
struct Entry<T> {
    left: Arc<[Vec3]>,
    right: Arc<[Vec3]>,
    points: T,
}
pub struct PointCache<T> {
    entries: VecDeque<Entry<T>>,
}
impl<T> Default for PointCache<T> {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }
}
impl<T> PointCache<T> {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// # Errors
    /// Returns coordinate conversion or retained graphics buffer creation errors.
    pub fn points_for(
        &mut self,
        strip: &MeshStrip,
        create: impl FnOnce(&[[f32; 3]]) -> Result<T>,
    ) -> Result<&T> {
        if let Some(index) = self.entries.iter().position(|entry| {
            Arc::ptr_eq(&entry.left, &strip.left) && Arc::ptr_eq(&entry.right, &strip.right)
        }) {
            self.entries.make_contiguous()[index..].rotate_left(1);
        } else {
            let count = strip.left.len().min(strip.right.len());
            let mut points = Vec::with_capacity(count * 2);
            for index in 0..count {
                for point in [strip.left[index], strip.right[index]] {
                    points.push([scalar(point.x)?, scalar(point.y)?, scalar(point.z)?]);
                }
            }
            let entry = Entry {
                left: Arc::clone(&strip.left),
                right: Arc::clone(&strip.right),
                points: create(&points)?,
            };
            self.entries.push_back(entry);
            while self.entries.len() > TRIANGLE_STRIP_POINT_CACHE_LIMIT {
                self.entries.pop_front();
            }
        }
        Ok(&self.entries[self.entries.len() - 1].points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn strip() -> MeshStrip {
        MeshStrip::new(
            vec![Vec3::new(0., 1., 2.), Vec3::new(3., 4., 5.)],
            vec![Vec3::new(6., 7., 8.), Vec3::new(9., 10., 11.)],
            [255; 4],
        )
    }
    #[test]
    fn identity_hits_preserve_source_lru_and_float_point_order() -> Result<()> {
        let mut cache = PointCache::default();
        let calls = Cell::new(0);
        let first = strip();
        let create = |points: &[[f32; 3]]| {
            calls.set(calls.get() + 1);
            Ok(points.to_vec())
        };
        assert_eq!(
            cache.points_for(&first, create)?,
            &[[0., 1., 2.], [6., 7., 8.], [3., 4., 5.], [9., 10., 11.]]
        );
        let mut same = first.clone();
        same.color = [0; 4];
        same.x_offset_m = 3.;
        cache.points_for(&same, create)?;
        assert_eq!(calls.get(), 1);
        for _ in 1..TRIANGLE_STRIP_POINT_CACHE_LIMIT {
            cache.points_for(&strip(), create)?;
        }
        cache.points_for(&first, create)?;
        cache.points_for(&strip(), create)?;
        assert_eq!(cache.len(), TRIANGLE_STRIP_POINT_CACHE_LIMIT);
        cache.points_for(&first, create)?;
        assert_eq!(calls.get(), TRIANGLE_STRIP_POINT_CACHE_LIMIT + 1);
        cache.clear();
        assert!(cache.is_empty());
        Ok(())
    }
}
