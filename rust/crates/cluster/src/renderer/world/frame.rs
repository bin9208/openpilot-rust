use super::DESIGN_WIDTH;
use crate::{
    models::ClusterUiState,
    renderer::{ClusterUiRenderer, Error, Result},
    scene::ClusterScene,
};
use num_traits::ToPrimitive;
impl ClusterUiRenderer {
    /// # Errors
    /// Returns resource/drawing errors; restores the viewport and matrix/mode stacks.
    pub fn draw_scene(&mut self, scene: &ClusterScene, state: &ClusterUiState) -> Result<()> {
        let shift = self.world_view_shift_x(state);
        let driving = if shift.abs() > 0.001 {
            self.driving_panel_offset_design_x() * self.width / DESIGN_WIDTH
        } else {
            0.
        };
        let offset = driving - shift;
        let width = self
            .width
            .to_i32()
            .ok_or(Error::Contract("world viewport width invalid"))?;
        let height = self
            .height
            .to_i32()
            .ok_or(Error::Contract("world viewport height invalid"))?;
        let translated = offset.abs() > 0.001;
        if translated {
            self.gfx.viewport(
                offset
                    .round_ties_even()
                    .to_i32()
                    .ok_or(Error::Contract("world viewport offset invalid"))?,
                0,
                width,
                height,
            )?;
        }
        let result = self.draw_scene_geometry(scene);
        let cleanup = if translated {
            self.gfx.viewport(0, 0, width, height)
        } else {
            Ok(())
        };
        result.and(cleanup)?;
        if translated {
            self.gfx.push_matrix()?;
        }
        let result = (|| -> Result<()> {
            if translated {
                self.gfx.translate(offset, 0., 0.)?;
            }
            let start = self.profile_start();
            self.draw_radar_point_labels(
                &scene.radar_points,
                &scene.camera,
                scene.scene_shift_x_m,
                state.radar_info_mode,
            )?;
            self.profile_add("draw_scene.radar_labels", start);
            let start = self.profile_start();
            self.draw_vehicle_badges(
                &scene.vehicles,
                &scene.camera,
                scene.scene_shift_x_m,
                state.radar_info_mode,
                state.radar_source_color_mode,
            )?;
            self.profile_add("draw_scene.vehicle_badges", start);
            Ok(())
        })();
        let cleanup = if translated {
            self.gfx.pop_matrix()
        } else {
            Ok(())
        };
        result.and(cleanup)
    }
    fn draw_scene_geometry(&mut self, scene: &ClusterScene) -> Result<()> {
        let start = self.profile_start();
        self.gfx.begin_mode3d(&scene.camera)?;
        self.profile_add("draw_scene.begin_mode_3d", start);
        let result = (|| -> Result<()> {
            self.gfx.push_matrix()?;
            let result = (|| -> Result<()> {
                if scene.scene_shift_x_m.abs() > 0.0001 {
                    self.gfx.translate(scene.scene_shift_x_m, 0., 0.)?;
                }
                for (name, strips) in [
                    ("draw_scene.highlight_lanes", &scene.highlight_lanes),
                    ("draw_scene.road_edges", &scene.road_edges),
                    ("draw_scene.lane_markings", &scene.lane_markings),
                    ("draw_scene.planned_path", &scene.planned_path),
                ] {
                    let start = self.profile_start();
                    for strip in strips {
                        self.draw_strip(strip)?;
                    }
                    self.profile_add(name, start);
                }
                let start = self.profile_start();
                for point in &scene.radar_points {
                    self.draw_radar_point(point)?;
                }
                self.profile_add("draw_scene.radar_points", start);
                let start = self.profile_start();
                for vehicle in &scene.vehicles {
                    self.draw_vehicle(vehicle)?;
                }
                self.profile_add("draw_scene.vehicles", start);
                Ok(())
            })();
            let cleanup = self.gfx.pop_matrix();
            result.and(cleanup)
        })();
        let start = self.profile_start();
        let cleanup = self.gfx.end_mode3d();
        self.profile_add("draw_scene.end_mode_3d", start);
        result.and(cleanup)
    }
}
