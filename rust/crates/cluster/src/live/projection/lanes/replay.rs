use super::{geometry::Geometry, Animation, Lanes};
use crate::models::{ClusterUiState, ModelPathPoint};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Snapshot {
    pub center: Option<f64>,
    pub inner: [f64; 2],
    pub visible: [bool; 2],
    pub extra: [bool; 2],
    pub edges: [Option<f64>; 2],
    pub styles: [String; 2],
    pub colors: [Option<[u8; 3]>; 2],
    pub lines: Vec<Vec<ModelPathPoint>>,
    pub model_edges: Vec<Vec<ModelPathPoint>>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            center: None,
            inner: [-0.5, 0.5],
            visible: [true; 2],
            extra: [false; 2],
            edges: [None; 2],
            styles: ["solid".into(), "solid".into()],
            colors: [None; 2],
            lines: Vec::new(),
            model_edges: Vec::new(),
        }
    }
}
impl Lanes {
    pub fn snapshot(&self, width: f64) -> Snapshot {
        let geometry = self.geometry(width);
        Snapshot {
            center: geometry.center,
            inner: geometry.inner,
            visible: geometry.visible,
            extra: geometry.extra,
            edges: geometry.edges,
            styles: self.styles.clone(),
            colors: self.colors,
            lines: self.points.clone(),
            model_edges: self.edges.clone(),
        }
    }
}
impl Snapshot {
    pub(crate) fn render(&self, state: &mut ClusterUiState, animation: &Animation) {
        let lanes = Lanes {
            points: self.lines.clone(),
            edges: self.model_edges.clone(),
            styles: self.styles.clone(),
            colors: self.colors,
            ..Lanes::default()
        };
        lanes.frame_geometry(
            state,
            animation,
            &Geometry {
                center: self.center,
                inner: self.inner,
                visible: self.visible,
                extra: self.extra,
                edges: self.edges,
            },
        );
    }
}
