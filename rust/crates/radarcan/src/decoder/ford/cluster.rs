use super::{Cluster, Ford};
use crate::{base::Base, clustering, numerics::Numerics, point::Point, scalar, Error};
use indexmap::IndexMap;

impl Ford {
    pub(super) fn publish(&mut self, base: &mut Base, numerics: &Numerics) -> Result<(), Error> {
        let previous = self
            .clusters
            .iter()
            .map(|c| [c.d_rel, c.y_rel * 2., c.v_rel * 2.])
            .collect::<Vec<_>>();
        let labels = clustering::correlate(&previous, &self.points, 5., numerics)?.labels;
        let mut groups: IndexMap<u64, Vec<[f64; 3]>> = IndexMap::new();
        for (point, label) in self.points.iter().zip(labels) {
            let id = if label != -1 {
                self.clusters[label as usize].track_id
            } else {
                let id = self.track_id;
                self.track_id += 1;
                id
            };
            groups.entry(id).or_default().push(*point);
        }
        self.clusters.clear();
        for (index, (id, points)) in groups.iter().enumerate() {
            let mut minimum = points[0][0];
            for point in &points[1..] {
                minimum = scalar::minimum(minimum, point[0]);
            }
            let count = points.len() as f64;
            let distance = scalar::float_sum(points.iter().map(|p| p[0])) / count;
            let lateral = scalar::float_sum(points.iter().map(|p| p[1])) / count / 2.;
            let velocity = scalar::float_sum(points.iter().map(|p| p[2])) / count / 2.;
            self.clusters.push(Cluster {
                d_rel: distance,
                y_rel: lateral,
                v_rel: velocity,
                track_id: *id,
            });
            let point = base.pts.entry(index as u64).or_insert_with(|| Point {
                measured: true,
                a_rel: f32::NAN,
                ..Point::default()
            });
            point.d_rel = minimum as f32;
            point.y_rel = lateral as f32;
            point.v_rel = velocity as f32;
            point.v_lead = (velocity + base.v_ego) as f32;
            point.track_id = *id;
        }
        for index in groups.len()..base.pts.len() {
            if base.pts.shift_remove(&(index as u64)).is_none() {
                return Err(Error::Contract("MRR tail point absent"));
            }
        }
        self.points.clear();
        Ok(())
    }
}
