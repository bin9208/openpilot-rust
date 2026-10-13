use super::common::number;
use crate::config::{AMBER, BLUE, BLUE_SOFT, WHITE};
use crate::models::NaviLiveState;
use crate::renderer::tokens::NAVI_MAP_BACKGROUND;
use crate::renderer::{Anchor, ClusterUiRenderer, Point, Rect, Result, StrokeStyle};

impl ClusterUiRenderer {
    pub(in crate::renderer) fn draw_navi_route_fallback(
        &mut self,
        navi: Option<&NaviLiveState>,
        rect: Rect,
    ) -> Result<()> {
        let theme = self.current_theme();
        self.gfx.rectangle(rect, [9, 13, 18])?;
        if let Some(points) = navi
            .and_then(|v| v.route.as_ref())
            .map(|v| v.polyline.as_slice())
            .filter(|v| v.len() >= 2)
        {
            let (min_lat, max_lat, min_lon, max_lon) = points.iter().fold(
                (points[0][0], points[0][0], points[0][1], points[0][1]),
                |(a, b, c, d), point| {
                    (
                        a.min(point[0]),
                        b.max(point[0]),
                        c.min(point[1]),
                        d.max(point[1]),
                    )
                },
            );
            let lat_span = (max_lat - min_lat).max(0.00001);
            let lon_span = (max_lon - min_lon).max(0.00001);
            let project = |latitude, longitude| {
                Point::new(
                    rect.x + 42.0 + (longitude - min_lon) / lon_span * (rect.width - 84.0),
                    rect.y + rect.height
                        - 42.0
                        - (latitude - min_lat) / lat_span * (rect.height - 84.0),
                )
            };
            let mut previous = project(points[0][0], points[0][1]);
            for point in &points[1..] {
                let current = project(point[0], point[1]);
                self.gfx.line(previous, current, 7.0, [21, 58, 96])?;
                self.gfx.line(previous, current, 3.0, BLUE_SOFT)?;
                previous = current;
            }
            if let Some(vehicle) = navi.and_then(|v| v.vehicle.as_ref()) {
                let point = project(vehicle.latitude, vehicle.longitude);
                self.gfx.circle(point, 11.0, WHITE)?;
                self.gfx.circle(point, 7.0, BLUE)?;
            }
        }
        self.draw_text(
            "MAP STREAM WAITING",
            rect.x + rect.width * 0.5,
            224.0,
            18.0,
            theme.muted,
            Anchor::Center,
        )
    }

    pub(in crate::renderer) fn draw_navi_map_stalled(
        &mut self,
        rect: Rect,
        age_ms: Option<i64>,
    ) -> Result<()> {
        self.gfx.rectangle(rect, NAVI_MAP_BACKGROUND)?;
        let x = rect.x + rect.width * 0.5;
        let y = rect.y + rect.height * 0.5;
        self.draw_text_with_stroke(
            "MAP STREAM STALLED",
            x,
            y - 18.0,
            24.0,
            AMBER,
            StrokeStyle {
                color: [5, 9, 12, 255],
                width: 2,
                anchor: Anchor::Center,
                cache: false,
            },
        )?;
        let age = age_ms.map_or_else(
            || "WAITING FOR NEXT FRAME".into(),
            |age| format!("NO MAP FRAME FOR {:.1} s", number(age) / 1000.0),
        );
        self.draw_text_with_stroke(
            &age,
            x,
            y + 18.0,
            16.0,
            WHITE,
            StrokeStyle {
                color: [5, 9, 12, 255],
                width: 1,
                anchor: Anchor::Center,
                cache: false,
            },
        )
    }
}
