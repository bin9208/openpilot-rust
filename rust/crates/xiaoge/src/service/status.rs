use super::{rounded, State};
use crate::{
    vision::Side,
    wire::{fields, object},
};
use openpilot_logging::{Fields, Value};

fn age(now: f64, received: f64) -> Value {
    if received == 0.0 {
        Value::Null
    } else {
        Value::Float(now - received)
    }
}

impl State {
    pub fn status(&self, now: f64, nanos: u64) -> Result<Fields, std::num::ParseFloatError> {
        let model = &self.models[0];
        let camera = &self.cameras[0];
        let road = &self.cameras[1];
        let configured: Vec<_> = [
            (&self.config.poly_left, "left"),
            (&self.config.poly_right, "right"),
        ]
        .into_iter()
        .filter(|(points, _)| points.len() >= 3)
        .map(|(_, name)| Value::Text(name.to_owned()))
        .collect();
        let sides = object([
            ("left", self.side_status(Side::Left, now, nanos)),
            ("right", self.side_status(Side::Right, now, nanos)),
        ]);
        let mut inference = self.inference_status(0, now)?;
        inference.insert(
            "updatedMonoTimeNanos".to_owned(),
            Value::Integer(i128::from(self.blindspot_updated)),
        );
        Ok(fields([
            ("standalone", Value::Bool(false)),
            ("integrated", Value::Bool(true)),
            (
                "model",
                object([
                    ("path", Value::Text(model.path.clone())),
                    ("loaded", Value::Bool(model.loaded)),
                    ("error", Value::Text(model.error.clone())),
                ]),
            ),
            ("configured", Value::Bool(true)),
            ("configuredSides", Value::Array(configured)),
            (
                "gate",
                object([
                    ("active", Value::Bool(self.gate.active)),
                    (
                        "side",
                        Value::Text(self.gate.side.map_or("", Side::name).to_owned()),
                    ),
                    ("reason", Value::Text(self.gate.reason.to_owned())),
                    ("laneWidth", Value::Float(self.gate.lane_width)),
                ]),
            ),
            ("threshold", Value::Float(self.settings.threshold)),
            (
                "smoothingSeconds",
                Value::Float(self.settings.smoothing_seconds),
            ),
            (
                "baseIntervalSeconds",
                Value::Float(self.settings.base_interval_seconds),
            ),
            (
                "camera",
                object([
                    ("available", Value::Bool(camera.available(now))),
                    ("error", Value::Text(camera.error.clone())),
                    ("lastFrameAgeSeconds", age(now, camera.last_frame)),
                ]),
            ),
            ("imageSide", sides.clone()),
            ("vehicleSide", sides),
            ("inference", Value::Object(inference)),
            (
                "lastInferenceAgeSeconds",
                age(now, self.metrics[0].last_inference),
            ),
            (
                "lane",
                object([
                    ("enabled", Value::Bool(true)),
                    ("loaded", Value::Bool(self.models[1].loaded)),
                    ("error", Value::Text(self.models[1].error.clone())),
                    ("threshold", Value::Float(self.settings.lane_threshold)),
                    (
                        "intervalSeconds",
                        Value::Float(self.settings.lane_interval_seconds),
                    ),
                    ("cameraAvailable", Value::Bool(road.available(now))),
                    ("cameraError", Value::Text(road.error.clone())),
                    ("lastFrameAgeSeconds", age(now, road.last_frame)),
                    ("resultFresh", Value::Bool(self.lane_fresh(now, nanos))),
                    ("result", self.lane_status()),
                    ("inference", Value::Object(self.inference_status(1, now)?)),
                ]),
            ),
        ]))
    }

    fn side_status(&self, side: Side, now: f64, nanos: u64) -> Value {
        let valid = self.side_valid(side, now, nanos);
        object([
            ("valid", Value::Bool(valid)),
            (
                "active",
                Value::Bool(valid && self.blindspot_active[side.index()]),
            ),
            (
                "confidence",
                Value::Float(if valid {
                    self.sides[side.index()].confidence
                } else {
                    0.0
                }),
            ),
        ])
    }

    fn inference_status(
        &self,
        stream: usize,
        now: f64,
    ) -> Result<Fields, std::num::ParseFloatError> {
        let metrics = &self.metrics[stream];
        let age = if metrics.last_inference == 0.0 {
            Value::Null
        } else {
            Value::Float(rounded(now - metrics.last_inference, 2)?)
        };
        Ok(fields([
            ("latencyMs", Value::Float(rounded(metrics.latency_ms, 1)?)),
            (
                "threadCpuMs",
                Value::Float(rounded(metrics.thread_cpu_ms, 1)?),
            ),
            ("fps", Value::Float(metrics.fps)),
            ("count", Value::Integer(i128::from(metrics.count))),
            ("lastAgeSeconds", age),
        ]))
    }

    fn lane_status(&self) -> Value {
        object([
            ("leftLine", Value::Integer(i128::from(self.lane.left_line))),
            (
                "rightLine",
                Value::Integer(i128::from(self.lane.right_line)),
            ),
            ("leftConf", Value::Float(self.lane.left_conf)),
            ("rightConf", Value::Float(self.lane.right_conf)),
            (
                "candidatesCount",
                Value::Integer(i128::from(self.lane.candidates_count)),
            ),
            ("valid", Value::Bool(self.lane.valid)),
            ("error", Value::Text(self.lane.error.clone())),
            (
                "updatedMonoTimeNanos",
                Value::Integer(i128::from(self.lane_updated)),
            ),
        ])
    }

    pub fn publication(&self, now: f64, nanos: u64) -> Result<Fields, std::num::ParseFloatError> {
        let side = [Side::Left, Side::Right]
            .into_iter()
            .find(|&side| self.side_valid(side, now, nanos));
        Ok(fields([
            ("type", Value::Text("xiaogeVision".to_owned())),
            ("version", Value::Integer(1)),
            (
                "lane",
                object([
                    ("leftLine", Value::Integer(i128::from(self.lane.left_line))),
                    (
                        "rightLine",
                        Value::Integer(i128::from(self.lane.right_line)),
                    ),
                    ("valid", Value::Bool(self.lane_fresh(now, nanos))),
                    (
                        "latencyMs",
                        Value::Float(rounded(self.metrics[1].latency_ms, 1)?),
                    ),
                    (
                        "receivedMonoTimeNanos",
                        Value::Integer(i128::from(if self.lane_updated == 0 {
                            nanos
                        } else {
                            self.lane_updated
                        })),
                    ),
                ]),
            ),
            (
                "blindspot",
                object([
                    (
                        "left",
                        Value::Bool(
                            self.side_valid(Side::Left, now, nanos) && self.blindspot_active[0],
                        ),
                    ),
                    (
                        "right",
                        Value::Bool(
                            self.side_valid(Side::Right, now, nanos) && self.blindspot_active[1],
                        ),
                    ),
                    ("valid", Value::Bool(side.is_some())),
                    ("side", Value::Text(side.map_or("", Side::name).to_owned())),
                    (
                        "receivedMonoTimeNanos",
                        Value::Integer(i128::from(if self.blindspot_updated == 0 {
                            nanos
                        } else {
                            self.blindspot_updated
                        })),
                    ),
                ]),
            ),
        ]))
    }
}
