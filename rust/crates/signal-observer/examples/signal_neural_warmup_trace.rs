//! Source warmup call order and every input byte, using a scripted session.
use openpilot_signal_observer::{
    neural::{Inference, Neural, SourceSort, Tensor},
    neural_sequence::Error,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, rc::Rc};
struct Session {
    name: &'static str,
    calls: Rc<RefCell<Vec<Value>>>,
}
impl Inference for Session {
    fn run(&mut self, name: &str, input: &Tensor) -> Result<Tensor, Error> {
        let mut digest = Sha256::new();
        for value in &input.values {
            digest.update(value.to_le_bytes());
        }
        self.calls.borrow_mut().push(json!({"network":self.name,"name":name,"shape":input.shape,"sha256":format!("{:x}",digest.finalize())}));
        Ok(Tensor {
            shape: Vec::new(),
            values: Vec::new(),
        })
    }
}
fn main() -> Result<(), Error> {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut network = Neural {
        detector: Session {
            name: "detector",
            calls: calls.clone(),
        },
        classifier: Session {
            name: "classifier",
            calls: calls.clone(),
        },
        sort: SourceSort,
    };
    network.warm_up()?;
    println!("{}", serde_json::to_string(&*calls.borrow())?);
    Ok(())
}
