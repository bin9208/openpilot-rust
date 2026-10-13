use super::{obj::ObjMesh, point_cache::PointCache};
use crate::renderer::{Graphics, ModelResource, PointBuffer3D, Result};
use std::{path::Path, time::Instant};

#[derive(Default)]
pub struct WorldResources {
    pub(super) model: Option<ModelResource>,
    pub(super) strips: PointCache<PointBuffer3D>,
    attempted: bool,
}
pub struct ModelLoadTiming {
    pub name: &'static str,
    pub elapsed_ms: f64,
}
impl WorldResources {
    /// # Errors
    /// Drawing backend errors follow the source's logged box fallback policy.
    pub fn load_model(
        &mut self,
        graphics: &mut Graphics,
        root: &Path,
    ) -> Result<Vec<ModelLoadTiming>> {
        if self.attempted {
            return Ok(Vec::new());
        }
        self.attempted = true;
        let path = root.join(
            "openpilot/selfdrive/carrot/cluster/assets/models/cybertruck/cybertruck_cluster.obj",
        );
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut timings = Vec::new();
        let result = (|| -> Result<ModelResource> {
            let start = Instant::now();
            let obj = ObjMesh::load(&path)?;
            let mesh = graphics.mesh(&obj.vertices, &obj.normals, &obj.colors)?;
            timings.push(ModelLoadTiming {
                name: "vehicle_model.parse_obj",
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
            });
            let start = Instant::now();
            graphics.upload_mesh(&mesh, false)?;
            timings.push(ModelLoadTiming {
                name: "vehicle_model.upload_mesh",
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
            });
            let start = Instant::now();
            let model = graphics.model_from_mesh(mesh)?;
            timings.push(ModelLoadTiming {
                name: "vehicle_model.load_from_mesh",
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
            });
            Ok(model)
        })();
        match result {
            Ok(model) => self.model = Some(model),
            Err(error) => println!("Cybertruck vehicle model load failed: {error}"),
        }
        Ok(timings)
    }
    pub fn close_resources(&mut self) {
        self.model = None;
        self.strips.clear();
        self.attempted = false;
    }
    #[must_use]
    pub fn model_loaded(&self) -> bool {
        self.model.is_some()
    }
    #[must_use]
    pub fn point_cache_len(&self) -> usize {
        self.strips.len()
    }
}

impl crate::renderer::ClusterUiRenderer {
    #[must_use]
    pub fn world_resources(&self) -> &WorldResources {
        &self.world
    }
}
