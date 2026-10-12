use super::{Data, Radar};
use crate::models::{DetectedVehicle, RADAR_MOVING_VEHICLE_MIN_SPEED_KPH};
use std::cmp::Ordering;

impl Radar {
    pub fn visible_detections(
        &self,
        model: &[DetectedVehicle],
        car: Data<'_>,
        time: f64,
        edges: [Option<f64>; 2],
    ) -> Vec<DetectedVehicle> {
        let mut detections = model.to_vec();
        if !self.front_only {
            let car_detections = car_corner_detections(car);
            for vehicle in &car_detections {
                if blocked(vehicle, edges) {
                    continue;
                }
                if !nearby(&detections, vehicle, 3.0, 1.1) {
                    detections.push(vehicle.clone());
                }
            }
            if let Some(index) = [1, 0]
                .into_iter()
                .find(|index| time - self.can_time[*index] < 0.8)
            {
                for vehicle in &self.can_detections[index] {
                    if (vehicle.label == "LR" && !car.flag("leftBlindspot", false))
                        || (vehicle.label == "RR" && !car.flag("rightBlindspot", false))
                        || car_detections
                            .iter()
                            .any(|value| value.label == vehicle.label)
                        || blocked(vehicle, edges)
                        || !inside(vehicle, edges)
                    {
                        continue;
                    }
                    if !nearby(&detections, vehicle, 3.0, 1.1) {
                        detections.push(vehicle.clone());
                    }
                }
            }
        }
        if time - self.detection_time < 0.8 {
            for vehicle in &self.detections {
                if vehicle.source == "radarState" && vehicle.cut_in {
                    detections.push(vehicle.clone());
                    continue;
                }
                if blocked(vehicle, edges) {
                    continue;
                }
                if !inside(vehicle, edges)
                    && (vehicle.source != "radarState"
                        || vehicle
                            .absolute_speed_kph
                            .is_none_or(|speed| speed.abs() < RADAR_MOVING_VEHICLE_MIN_SPEED_KPH))
                {
                    continue;
                }
                if vehicle.source == "radarState" || !nearby(&detections, vehicle, 4.0, 1.4) {
                    detections.push(vehicle.clone());
                }
            }
        }
        detections.sort_by(|left, right| {
            left.longitudinal_m
                .partial_cmp(&right.longitudinal_m)
                .unwrap_or(Ordering::Equal)
        });
        detections
    }
}
fn nearby(
    vehicles: &[DetectedVehicle],
    candidate: &DetectedVehicle,
    longitudinal: f64,
    lateral: f64,
) -> bool {
    vehicles.iter().any(|vehicle| {
        (vehicle.longitudinal_m - candidate.longitudinal_m).abs() <= longitudinal
            && (vehicle.lateral_m - candidate.lateral_m).abs() <= lateral
    })
}
fn inside(vehicle: &DetectedVehicle, edges: [Option<f64>; 2]) -> bool {
    !edges[0].is_some_and(|edge| vehicle.lateral_m < edge - 0.25)
        && !edges[1].is_some_and(|edge| vehicle.lateral_m > edge + 0.25)
}
fn blocked(vehicle: &DetectedVehicle, edges: [Option<f64>; 2]) -> bool {
    vehicle.longitudinal_m.abs() <= 10.0 && !inside(vehicle, edges)
}
fn car_corner_detections(car: Data<'_>) -> Vec<DetectedVehicle> {
    let mut detections = Vec::new();
    for (label, distance, lateral, side, forward) in [
        ("LF", "leftLongDist", "leftLatDist", -1.0, 1.0),
        ("RF", "rightLongDist", "rightLatDist", 1.0, 1.0),
        ("LR", "leftRearLongDist", "leftRearLatDist", -1.0, -1.0),
        ("RR", "rightRearLongDist", "rightRearLatDist", 1.0, -1.0),
    ] {
        if label == "LR" && !car.flag("leftBlindspot", false)
            || label == "RR" && !car.flag("rightBlindspot", false)
        {
            continue;
        }
        let distance = car.number(distance, 0.0);
        if distance <= 0.2 || distance >= 180.0 {
            continue;
        }
        let magnitude = car.number(lateral, 0.0);
        let magnitude = if (0.4..=6.0).contains(&magnitude) {
            magnitude
        } else {
            3.0
        };
        detections.push(DetectedVehicle {
            label: label.into(),
            longitudinal_m: forward * distance,
            lateral_m: side * magnitude,
            source: "carState".into(),
            ..DetectedVehicle::default()
        });
    }
    detections
}
