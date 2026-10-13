use super::{
    helpers::{rectangles_overlap, Bounds},
    vehicle_has_lead_one_metrics,
};
use crate::{
    renderer::{Anchor, ClusterUiRenderer, Error, Rect, Result},
    scene::{math::clamp, VehicleBox},
};
#[derive(Clone, Debug)]
pub struct MetricLine {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
}
#[derive(Clone)]
pub struct MetricLabel<'a> {
    pub vehicle: &'a VehicleBox,
    pub lines: Vec<MetricLine>,
    pub color: [u8; 4],
    pub shadow_offset: f64,
}
#[must_use]
pub fn metric_lines(
    distance: String,
    speed: String,
    x: f64,
    y: f64,
    sizes: (f64, f64),
    scale: f64,
) -> Vec<MetricLine> {
    let (distance_size, speed_size) = sizes;
    let gap = (4. * scale).max(2.);
    let speed_y = y - speed_size * 0.5;
    let distance_y = if speed.is_empty() {
        y - distance_size * 0.5
    } else if distance.is_empty() {
        0.
    } else {
        speed_y - (speed_size + distance_size) * 0.5 - gap
    };
    let mut lines = Vec::new();
    if !distance.is_empty() {
        lines.push(MetricLine {
            text: distance,
            x,
            y: distance_y,
            size: distance_size,
        });
    }
    if !speed.is_empty() {
        lines.push(MetricLine {
            text: speed,
            x,
            y: speed_y,
            size: speed_size,
        });
    }
    lines
}
impl ClusterUiRenderer {
    /// # Errors
    /// Returns font measurement errors or an empty label contract error.
    pub fn vehicle_metric_label_rect(&mut self, label: &MetricLabel<'_>) -> Result<Rect> {
        let mut bounds: Option<Bounds> = None;
        for line in &label.lines {
            let (width, height) =
                self.measure_text(&line.text, line.size, Some((line.size * 0.02).max(1.)))?;
            let current = Bounds::new(
                line.x - width * 0.5,
                line.y - height * 0.5,
                line.x + width * 0.5,
                line.y + height * 0.5,
            );
            bounds = Some(bounds.map_or(current, |old| {
                Bounds::new(
                    old.left.min(current.left),
                    old.top.min(current.top),
                    old.right.max(current.right),
                    old.bottom.max(current.bottom),
                )
            }));
        }
        let bounds = bounds.ok_or(Error::Contract("vehicle metric label has no lines"))?;
        Ok(Rect::new(
            bounds.left,
            bounds.top,
            bounds.right - bounds.left + label.shadow_offset,
            bounds.bottom - bounds.top + label.shadow_offset,
        ))
    }
    /// # Errors
    /// Returns font measurement or world label drawing errors.
    pub fn draw_vehicle_metric_labels(
        &mut self,
        labels: &[MetricLabel<'_>],
        bounds: Option<Bounds>,
    ) -> Result<()> {
        let mut lead = labels
            .iter()
            .find(|label| vehicle_has_lead_one_metrics(label.vehicle))
            .cloned();
        let mut protected = None;
        if let Some(label) = lead.as_mut() {
            let bounds = bounds.unwrap_or_else(|| Bounds::new(0., 0., self.width, self.height));
            let rect = self.vehicle_metric_label_rect(label)?;
            let dx = clamp(
                rect.x,
                bounds.left + 4.,
                (bounds.right - rect.width - 4.).max(bounds.left + 4.),
            ) - rect.x;
            let dy = clamp(
                rect.y,
                bounds.top + 4.,
                (bounds.bottom - rect.height - 4.).max(bounds.top + 4.),
            ) - rect.y;
            for line in &mut label.lines {
                line.x += dx;
                line.y += dy;
            }
            protected = Some(Rect::new(
                rect.x + dx - 4.,
                rect.y + dy - 4.,
                rect.width + 8.,
                rect.height + 8.,
            ));
        }
        let mut visible = Vec::new();
        for label in labels {
            if lead
                .as_ref()
                .is_some_and(|lead| std::ptr::eq(label.vehicle, lead.vehicle))
            {
                continue;
            }
            if let Some(protected) = protected {
                if rectangles_overlap(self.vehicle_metric_label_rect(label)?, protected) {
                    continue;
                }
            }
            visible.push(label.clone());
        }
        visible.extend(lead);
        let shadow = self.current_theme().world_label_shadow;
        for label in visible {
            for line in &label.lines {
                if label.shadow_offset != 0. {
                    self.draw_world_label_text(
                        &line.text,
                        line.x + label.shadow_offset,
                        line.y + label.shadow_offset,
                        line.size,
                        shadow,
                        Anchor::Center,
                    )?;
                }
                self.draw_world_label_text(
                    &line.text,
                    line.x,
                    line.y,
                    line.size,
                    label.color,
                    Anchor::Center,
                )?;
            }
        }
        Ok(())
    }
    #[must_use]
    pub fn world_label_bounds(&self, left: f64, top: f64, right: f64, bottom: f64) -> Bounds {
        super::helpers::world_label_bounds(
            (self.width, self.height),
            Bounds::new(left, top, right, bottom),
        )
    }
}
