use super::format;
use crate::{Error, Value};

struct Span {
    start: f64,
    end: f64,
    peak: f64,
}
pub(super) struct Excursions {
    accel: bool,
    over: f64,
    hard: f64,
    raw: Vec<Span>,
    current: Option<Span>,
}
impl Excursions {
    pub fn new(accel: bool, over: f64, hard: f64) -> Self {
        Self {
            accel,
            over,
            hard,
            raw: Vec::new(),
            current: None,
        }
    }
    fn peak(&self, first: f64, second: f64) -> f64 {
        if if self.accel {
            second > first
        } else {
            second < first
        } {
            second
        } else {
            first
        }
    }
    pub fn feed(&mut self, time: f64, value: f64) {
        let inside = if self.accel {
            value >= self.over
        } else {
            value <= self.over
        };
        if inside {
            let peak = self
                .current
                .as_ref()
                .map_or(value, |current| self.peak(current.peak, value));
            match &mut self.current {
                Some(current) => {
                    current.end = time;
                    current.peak = peak;
                }
                None => {
                    self.current = Some(Span {
                        start: time,
                        end: time,
                        peak,
                    })
                }
            }
        } else if let Some(current) = self.current.take() {
            self.raw.push(current);
        }
    }
    pub fn events(mut self) -> Result<(Value, Value), Error> {
        if let Some(current) = self.current.take() {
            self.raw.push(current);
        }
        let mut merged: Vec<Span> = Vec::new();
        for span in &self.raw {
            if let Some(last) = merged.last_mut().filter(|last| span.start - last.end < 1.5) {
                last.end = span.end;
                last.peak = self.peak(last.peak, span.peak);
            } else {
                merged.push(Span {
                    start: span.start,
                    end: span.end,
                    peak: span.peak,
                });
            }
        }
        let (mut hard, mut over) = (Vec::new(), Vec::new());
        for span in merged {
            let row = Value::object([
                ("clock", Value::text(&format::clock(span.start)?)),
                ("peak", format::number(span.peak, 2)),
            ]);
            if if self.accel {
                span.peak >= self.hard
            } else {
                span.peak <= self.hard
            } {
                hard.push(row);
            } else {
                over.push(row);
            }
        }
        let category = |mut items: Vec<Value>| {
            let count = items.len();
            items.truncate(60);
            Value::object([
                ("count", Value::integer(count)),
                ("items", Value::Array(items)),
            ])
        };
        Ok((category(hard), category(over)))
    }
}
