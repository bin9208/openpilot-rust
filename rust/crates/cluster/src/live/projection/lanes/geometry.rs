use super::Lanes;

pub(super) struct Geometry {
    pub center: Option<f64>,
    pub inner: [f64; 2],
    pub visible: [bool; 2],
    pub extra: [bool; 2],
    pub edges: [Option<f64>; 2],
}
pub(super) fn inside(offset: f64, edges: [Option<f64>; 2]) -> bool {
    !edges[0].is_some_and(|edge| offset < edge) && !edges[1].is_some_and(|edge| offset > edge)
}
impl Lanes {
    pub fn edge_bounds(&self, width: f64) -> [Option<f64>; 2] {
        let geometry = self.geometry(width);
        geometry.edges.map(|edge| {
            edge.zip(geometry.center)
                .map(|(offset, center)| center + offset * width.max(0.1))
        })
    }
    pub fn center(&self) -> Option<f64> {
        self.positions[1]
            .zip(self.positions[2])
            .filter(|(left, right)| right > left)
            .map(|(left, right)| (left + right) * 0.5)
    }
    pub(super) fn geometry(&self, width: f64) -> Geometry {
        let Some(center) = self.center() else {
            return Geometry {
                center: None,
                inner: [-0.5, 0.5],
                visible: [self.probabilities[1] >= 0.4, self.probabilities[2] >= 0.4],
                extra: [false; 2],
                edges: [None; 2],
            };
        };
        let width = width.clamp(2.4, 4.6);
        let offset = |position: Option<f64>| position.map(|value| (value - center) / width);
        let left = offset(self.edge_positions[0])
            .filter(|edge| *edge < -0.68)
            .map(|edge| edge.clamp(-2.8, -0.68));
        let right = offset(self.edge_positions[1])
            .filter(|edge| *edge > 0.68)
            .map(|edge| edge.clamp(0.68, 2.8));
        let edges = if left.zip(right).is_some_and(|(left, right)| left >= right) {
            [None; 2]
        } else {
            [left, right]
        };
        let inner = [
            offset(self.positions[1])
                .unwrap_or(-0.5)
                .clamp(-0.75, -0.25),
            offset(self.positions[2]).unwrap_or(0.5).clamp(0.25, 0.75),
        ];
        Geometry {
            center: Some(center),
            inner,
            visible: [
                self.probabilities[1] >= 0.4 && inside(inner[0], edges),
                self.probabilities[2] >= 0.4 && inside(inner[1], edges),
            ],
            extra: [
                offset(self.positions[0]).is_some_and(|offset| {
                    offset < -0.78 && self.probabilities[0] >= 0.4 && inside(offset, edges)
                }),
                offset(self.positions[3]).is_some_and(|offset| {
                    offset > 0.78 && self.probabilities[3] >= 0.4 && inside(offset, edges)
                }),
            ],
            edges,
        }
    }
}
