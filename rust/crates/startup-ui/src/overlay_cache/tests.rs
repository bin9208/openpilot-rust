use super::*;
use serde_json::{json, Value};
struct Target {
    id: u32,
    texture: u32,
    size: (i32, i32),
}
struct Graphics {
    active: u32,
    serial: u32,
    allocation: String,
    trace: std::rc::Rc<std::cell::RefCell<Vec<Value>>>,
}
impl Backend for Graphics {
    type Target = Target;
    type Error = ();
    fn framebuffer(&mut self) -> u32 {
        self.active
    }
    fn flush(&mut self) {
        self.trace.borrow_mut().push(json!(["flush"]));
    }
    fn allocate(&mut self, size: (i32, i32)) -> Result<Target, ()> {
        self.serial += 2;
        let id = if self.allocation == "zero" {
            0
        } else {
            self.serial
        };
        let texture = if self.allocation == "texture" {
            0
        } else {
            self.serial + 1
        };
        self.trace
            .borrow_mut()
            .push(json!(["allocate", size.0, size.1, id, texture]));
        self.active = 0;
        Ok(Target { id, texture, size })
    }
    fn complete(&mut self, target: &Target) -> Result<bool, ()> {
        if target.id == 0 || target.texture == 0 {
            return Ok(false);
        }
        self.trace.borrow_mut().push(json!(["complete", target.id]));
        Ok(self.allocation != "incomplete")
    }
    fn release(&mut self, target: Target) -> Result<(), ()> {
        self.trace.borrow_mut().push(json!(["release", target.id]));
        Ok(())
    }
    fn bind(&mut self, id: u32) {
        self.active = id;
        self.trace.borrow_mut().push(json!(["bind", id]));
    }
    fn target_framebuffer(&mut self, target: &Target) -> Result<u32, ()> {
        Ok(target.id)
    }
    fn begin_coverage(&mut self) {
        self.trace.borrow_mut().extend([
            json!(["clear", [0, 0, 0, 0]]),
            json!(["separate", 770, 771, 1, 771, 32774, 32774]),
            json!(["blend", 7]),
        ]);
    }
    fn end_coverage(&mut self) {
        self.trace.borrow_mut().push(json!(["end"]));
    }
    fn composite(&mut self, target: &Target, logical: (f32, f32)) -> Result<(), ()> {
        self.trace.borrow_mut().extend([
            json!(["blend", 5]),
            json!([
                "texture",
                target.texture,
                [0, 0, target.size.0, -target.size.1],
                [0, 0, logical.0, logical.1],
                [0, 0],
                0,
                [255, 255, 255, 255]
            ]),
            json!(["end"]),
        ]);
        Ok(())
    }
}
// JSON numeric representation is irrelevant to the C float/int graphics ABI.
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|b| equivalent(v, b)))
        }
        _ => a == b,
    }
}
#[test]
fn original_layer_all_state_and_framebuffer_calls() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/overlay-layer-final.json"
    ))
    .unwrap();
    for (sequence, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let mut layer = Layer::<Target, Value>::new(row["phase"].as_i64().unwrap());
        let mut graphics = Graphics {
            active: 73,
            serial: 80,
            allocation: "valid".into(),
            trace: Default::default(),
        };
        for (index, step) in row["steps"].as_array().unwrap().iter().enumerate() {
            graphics.trace.borrow_mut().clear();
            graphics.active = step["previous"].as_u64().unwrap() as u32;
            graphics.allocation = step["allocation"].as_str().unwrap().into();
            let mut rect = [0.0; 4];
            for (a, b) in rect.iter_mut().zip(step["rect"].as_array().unwrap()) {
                *a = b.as_f64().unwrap() as f32;
            }
            let clock_trace = graphics.trace.clone();
            let result = match step["op"].as_str().unwrap() {
                "invalidate" => {
                    layer.invalidate();
                    Ok(())
                }
                "close" => layer.close(&mut graphics),
                _ => layer.draw(
                    &mut graphics,
                    rect,
                    step["key"].clone(),
                    Frame {
                        pixels: (
                            step["pixels"][0].as_i64().unwrap() as i32,
                            step["pixels"][1].as_i64().unwrap() as i32,
                        ),
                        logical: (
                            step["logical"][0].as_f64().unwrap() as f32,
                            step["logical"][1].as_f64().unwrap() as f32,
                        ),
                        index: step["frame"].as_i64().unwrap(),
                        enabled: step["enabled"].as_bool().unwrap(),
                        urgent: step["urgent"].as_bool().unwrap(),
                    },
                    || {
                        clock_trace.borrow_mut().push(json!(["clock"]));
                        f64::from_bits(step["clock_bits"].as_u64().unwrap())
                    },
                    |g| {
                        g.trace.borrow_mut().push(json!(["paint", rect, g.active]));
                        if step["paint_error"].as_bool().unwrap() {
                            Err(())
                        } else {
                            Ok(())
                        }
                    },
                ),
            };
            assert_eq!(
                result.is_err(),
                step["error"].as_bool().unwrap(),
                "{sequence}/{index}"
            );
            assert!(
                equivalent(&json!(*graphics.trace.borrow()), &step["trace"]),
                "trace {sequence}/{index}: {:?} != {:?}",
                graphics.trace.borrow(),
                step["trace"]
            );
            assert_eq!(
                layer.painted_at.to_bits(),
                step["state"]["painted_at_bits"].as_u64().unwrap()
            );
            let key = layer
                .key
                .as_ref()
                .map(|(r, k)| json!([r[0], r[1], r[2], r[3], k]));
            let state = json!({"target":layer.target.as_ref().map(|t|t.id),"size":layer.size,"key":key,"frame":layer.frame,"painted_at_bits":layer.painted_at.to_bits(),"ready":layer.ready,"failed":layer.failed,"destination":[0,0,layer.destination.0,layer.destination.1]});
            assert!(
                equivalent(&state, &step["state"]),
                "state {sequence}/{index}: {state} != {}",
                step["state"]
            );
            assert_eq!(graphics.active as u64, step["active"].as_u64().unwrap());
        }
    }
}
