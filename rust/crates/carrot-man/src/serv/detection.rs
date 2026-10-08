use super::{CarrotServ, Detection};

impl CarrotServ {
    fn push_detection(&mut self, detection: Detection) {
        if self.detection.len() == 20 {
            self.detection.pop_front();
        }
        self.detection.push_back(detection);
    }
    pub fn traffic_light(&mut self, incoming: Detection) {
        let (mut red, mut green, mut left, mut red_trig, mut green_trig, mut left_trig) =
            (0., 0., 0., 0., 0., 0.);
        for previous in &self.detection {
            if (incoming.x - previous.x).abs() < 0.2 && (incoming.y - previous.y).abs() < 0.2 {
                match previous.color.as_str() {
                    "Green Light" | "Left turn" => match incoming.color.as_str() {
                        "Red Light" | "Yellow Light" => {
                            red_trig += incoming.confidence;
                            red += incoming.confidence;
                        }
                        "Green Light" | "Left turn" => green += incoming.confidence,
                        _ => {}
                    },
                    "Red Light" | "Yellow Light" => match incoming.color.as_str() {
                        "Green Light" => {
                            green_trig += incoming.confidence;
                            green += incoming.confidence;
                        }
                        "Left turn" => {
                            left_trig += incoming.confidence;
                            left += incoming.confidence;
                        }
                        "Red Light" | "Yellow Light" => red += incoming.confidence,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        self.traffic_state = if red_trig > 0. {
            1
        } else if green_trig > 0. && green > red {
            2
        } else if left_trig > 0. {
            3
        } else if red > 0. {
            1
        } else if green > 0. {
            2
        } else {
            0
        };
        let _ = left;
        self.push_detection(incoming);
    }
    pub fn update_command(&mut self) {
        self.command.handler_failed = false;
        if self.command.command_index != self.command.last_command_index {
            self.command.last_command_index = self.command.command_index;
            if !self.command.command_hashable
                || (self.command.command == "DETECT" && !self.command.argument_text)
            {
                self.command.handler_failed = true;
                return;
            }
            if self.command.command == "DETECT" {
                let values: Vec<_> = self.command.argument.split(',').map(str::trim).collect();
                if values.len() >= 4 {
                    if let (Some(x), Some(y), Some(confidence)) = (
                        openpilot_runtime_core::python_float::parse(values[1]),
                        openpilot_runtime_core::python_float::parse(values[2]),
                        openpilot_runtime_core::python_float::parse(values[3]),
                    ) {
                        self.traffic_light(Detection {
                            color: values[0].into(),
                            x,
                            y,
                            confidence,
                        });
                        self.traffic_light_count = 5;
                    }
                }
            }
        }
        self.push_detection(Detection {
            x: -1.,
            y: -1.,
            color: "none".into(),
            confidence: 0.,
        });
        self.traffic_light_count -= 1;
        if self.traffic_light_count < 0 {
            self.traffic_light_count = -1;
            self.traffic_state = 0;
        }
    }
}
