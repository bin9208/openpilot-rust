use std::collections::{BTreeMap, HashMap};

pub(super) type Id = usize;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Node {
    Constant(u64),
    State(usize),
    Global(usize),
    Dt,
    Add(Vec<Id>),
    Mul(Vec<Id>),
    Div(Id, Id),
    PowI(Id, i32),
    Sin(Id),
    Cos(Id),
    Asin(Id),
    Atan2(Id, Id),
    Sqrt(Id),
}

#[derive(Default)]
pub(super) struct Expressions {
    nodes: Vec<Node>,
    interned: BTreeMap<Node, Id>,
    derivatives: HashMap<(Id, usize), Id>,
}

impl Expressions {
    fn intern(&mut self, node: Node) -> Id {
        if let Some(&id) = self.interned.get(&node) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(node.clone());
        self.interned.insert(node, id);
        id
    }

    pub(super) fn constant(&mut self, value: f64) -> Id {
        self.intern(Node::Constant(value.to_bits()))
    }

    pub(super) fn state(&mut self, index: usize) -> Id {
        self.intern(Node::State(index))
    }

    pub(super) fn global(&mut self, index: usize) -> Id {
        self.intern(Node::Global(index))
    }

    pub(super) fn dt(&mut self) -> Id {
        self.intern(Node::Dt)
    }

    fn number(&self, id: Id) -> Option<f64> {
        match &self.nodes[id] {
            Node::Constant(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }

    pub(super) fn is_zero(&self, id: Id) -> bool {
        self.number(id) == Some(0.0)
    }

    fn is_one(&self, id: Id) -> bool {
        self.number(id) == Some(1.0)
    }

    pub(super) fn add(&mut self, parts: impl IntoIterator<Item = Id>) -> Id {
        let mut terms = Vec::new();
        for id in parts {
            if self.is_zero(id) {
                continue;
            }
            if let Node::Add(nested) = &self.nodes[id] {
                terms.extend(nested.iter().copied());
            } else {
                terms.push(id);
            }
        }
        match terms.len() {
            0 => self.constant(0.0),
            1 => terms[0],
            _ => self.intern(Node::Add(terms)),
        }
    }

    pub(super) fn mul(&mut self, parts: impl IntoIterator<Item = Id>) -> Id {
        let mut terms = Vec::new();
        for id in parts {
            if self.is_zero(id) {
                return self.constant(0.0);
            }
            if self.is_one(id) {
                continue;
            }
            if let Node::Mul(nested) = &self.nodes[id] {
                terms.extend(nested.iter().copied());
            } else {
                terms.push(id);
            }
        }
        match terms.len() {
            0 => self.constant(1.0),
            1 => terms[0],
            _ => self.intern(Node::Mul(terms)),
        }
    }

    pub(super) fn neg(&mut self, value: Id) -> Id {
        let minus_one = self.constant(-1.0);
        self.mul([minus_one, value])
    }

    pub(super) fn sub(&mut self, left: Id, right: Id) -> Id {
        let negative = self.neg(right);
        self.add([left, negative])
    }

    pub(super) fn div(&mut self, numerator: Id, denominator: Id) -> Id {
        if self.is_zero(numerator) {
            return self.constant(0.0);
        }
        if self.is_one(denominator) {
            return numerator;
        }
        self.intern(Node::Div(numerator, denominator))
    }

    pub(super) fn powi(&mut self, value: Id, exponent: i32) -> Id {
        match exponent {
            0 => self.constant(1.0),
            1 => value,
            _ => self.intern(Node::PowI(value, exponent)),
        }
    }

    pub(super) fn sin(&mut self, value: Id) -> Id {
        self.intern(Node::Sin(value))
    }

    pub(super) fn cos(&mut self, value: Id) -> Id {
        self.intern(Node::Cos(value))
    }

    pub(super) fn asin(&mut self, value: Id) -> Id {
        self.intern(Node::Asin(value))
    }

    pub(super) fn atan2(&mut self, y: Id, x: Id) -> Id {
        self.intern(Node::Atan2(y, x))
    }

    pub(super) fn sqrt(&mut self, value: Id) -> Id {
        self.intern(Node::Sqrt(value))
    }

    // For a denominator such as mass * speed, keep the state-independent
    // factor outside the squared varying factor. The frozen generated model
    // does the same cancellation; without it, mass=0 evaluates as 0/0.
    fn constant_times_varying(
        &mut self,
        denominator: Id,
        state_index: usize,
    ) -> Option<(Id, Id, Id)> {
        let Node::Mul(parts) = self.nodes[denominator].clone() else {
            return None;
        };
        let mut constant = Vec::new();
        let mut varying = None;
        for part in parts {
            let derivative = self.derivative(part, state_index);
            if self.is_zero(derivative) {
                constant.push(part);
            } else if varying.replace((part, derivative)).is_some() {
                return None;
            }
        }
        let (variable, derivative) = varying?;
        Some((self.mul(constant), variable, derivative))
    }

    pub(super) fn derivative(&mut self, id: Id, state_index: usize) -> Id {
        if let Some(&cached) = self.derivatives.get(&(id, state_index)) {
            return cached;
        }
        let result = match self.nodes[id].clone() {
            Node::Constant(_) | Node::Global(_) | Node::Dt => self.constant(0.0),
            Node::State(index) => self.constant(if index == state_index { 1.0 } else { 0.0 }),
            Node::Add(terms) => {
                let derivatives = terms
                    .into_iter()
                    .map(|term| self.derivative(term, state_index))
                    .collect::<Vec<_>>();
                self.add(derivatives)
            }
            Node::Mul(terms) => {
                let mut sum = Vec::new();
                for (index, &term) in terms.iter().enumerate() {
                    let derivative = self.derivative(term, state_index);
                    if !self.is_zero(derivative) {
                        let mut product = terms.clone();
                        product[index] = derivative;
                        sum.push(self.mul(product));
                    }
                }
                self.add(sum)
            }
            Node::Div(top, bottom) => {
                let d_top = self.derivative(top, state_index);
                let d_bottom = self.derivative(bottom, state_index);
                if self.is_zero(d_bottom) {
                    self.div(d_top, bottom)
                } else if self.is_zero(d_top) {
                    let (right, denominator) = if let Some((constant, variable, derivative)) =
                        self.constant_times_varying(bottom, state_index)
                    {
                        let squared = self.powi(variable, 2);
                        (self.mul([top, derivative]), self.mul([constant, squared]))
                    } else {
                        (self.mul([top, d_bottom]), self.powi(bottom, 2))
                    };
                    let negative = self.neg(right);
                    self.div(negative, denominator)
                } else {
                    let left = self.mul([d_top, bottom]);
                    let right = self.mul([top, d_bottom]);
                    let numerator = self.sub(left, right);
                    let denominator = self.powi(bottom, 2);
                    self.div(numerator, denominator)
                }
            }
            Node::PowI(base, exponent) => {
                let factor = self.constant(f64::from(exponent));
                let powered = self.powi(base, exponent - 1);
                let derivative = self.derivative(base, state_index);
                self.mul([factor, powered, derivative])
            }
            Node::Sin(value) => {
                let cosine = self.cos(value);
                let derivative = self.derivative(value, state_index);
                self.mul([cosine, derivative])
            }
            Node::Cos(value) => {
                let sine = self.sin(value);
                let negative = self.neg(sine);
                let derivative = self.derivative(value, state_index);
                self.mul([negative, derivative])
            }
            Node::Asin(value) => {
                let derivative = self.derivative(value, state_index);
                let one = self.constant(1.0);
                let square = self.powi(value, 2);
                let inner = self.sub(one, square);
                let denominator = self.sqrt(inner);
                self.div(derivative, denominator)
            }
            Node::Atan2(y, x) => {
                let dy = self.derivative(y, state_index);
                let dx = self.derivative(x, state_index);
                let left = self.mul([x, dy]);
                let right = self.mul([y, dx]);
                let numerator = self.sub(left, right);
                let x2 = self.powi(x, 2);
                let y2 = self.powi(y, 2);
                let denominator = self.add([x2, y2]);
                self.div(numerator, denominator)
            }
            Node::Sqrt(value) => {
                let derivative = self.derivative(value, state_index);
                let two = self.constant(2.0);
                let root = self.sqrt(value);
                let denominator = self.mul([two, root]);
                self.div(derivative, denominator)
            }
        };
        self.derivatives.insert((id, state_index), result);
        result
    }

    pub(super) fn render(&self, id: Id) -> String {
        match &self.nodes[id] {
            Node::Constant(bits) => format!("{:?}_f64", f64::from_bits(*bits)),
            Node::State(index) => format!("state[{index}]"),
            Node::Global(index) => format!("globals[{index}]"),
            Node::Dt => "dt".to_owned(),
            Node::Add(parts) => format!(
                "({})",
                parts
                    .iter()
                    .map(|part| self.render(*part))
                    .collect::<Vec<_>>()
                    .join(" + ")
            ),
            Node::Mul(parts) => format!(
                "({})",
                parts
                    .iter()
                    .map(|part| self.render(*part))
                    .collect::<Vec<_>>()
                    .join(" * ")
            ),
            Node::Div(top, bottom) => {
                format!("({} / {})", self.render(*top), self.render(*bottom))
            }
            Node::PowI(value, exponent) => format!("({}).powi({exponent})", self.render(*value)),
            Node::Sin(value) => format!("({}).sin()", self.render(*value)),
            Node::Cos(value) => format!("({}).cos()", self.render(*value)),
            Node::Asin(value) => format!("({}).asin()", self.render(*value)),
            Node::Atan2(y, x) => format!("({}).atan2({})", self.render(*y), self.render(*x)),
            Node::Sqrt(value) => format!("({}).sqrt()", self.render(*value)),
        }
    }

    pub(super) fn evaluate_many(
        &self,
        ids: &[Id],
        state: &[f64],
        globals: &[f64],
        dt: f64,
    ) -> Vec<f64> {
        let mut cache = vec![None; self.nodes.len()];
        ids.iter()
            .map(|id| self.evaluate_cached(*id, state, globals, dt, &mut cache))
            .collect()
    }

    fn evaluate_cached(
        &self,
        id: Id,
        state: &[f64],
        globals: &[f64],
        dt: f64,
        cache: &mut [Option<f64>],
    ) -> f64 {
        if let Some(value) = cache[id] {
            return value;
        }
        let mut at = |child| self.evaluate_cached(child, state, globals, dt, cache);
        let value = match &self.nodes[id] {
            Node::Constant(bits) => f64::from_bits(*bits),
            Node::State(index) => state[*index],
            Node::Global(index) => globals[*index],
            Node::Dt => dt,
            Node::Add(parts) => parts.iter().map(|part| at(*part)).sum(),
            Node::Mul(parts) => parts.iter().map(|part| at(*part)).product(),
            Node::Div(top, bottom) => at(*top) / at(*bottom),
            Node::PowI(child, exponent) => at(*child).powi(*exponent),
            Node::Sin(child) => at(*child).sin(),
            Node::Cos(child) => at(*child).cos(),
            Node::Asin(child) => at(*child).asin(),
            Node::Atan2(y, x) => at(*y).atan2(at(*x)),
            Node::Sqrt(child) => at(*child).sqrt(),
        };
        cache[id] = Some(value);
        value
    }
}
