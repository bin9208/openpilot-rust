use std::collections::VecDeque;
#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub time: f64,
    pub desired: f64,
    pub actual: f64,
    pub okay: bool,
}
pub struct Points {
    pub rows: VecDeque<Point>,
}
impl Points {
    pub fn new(count: usize) -> Self {
        Self {
            rows: std::iter::repeat_n(Point::default(), count).collect(),
        }
    }
    pub fn update(&mut self, point: Point) {
        if !self.rows.is_empty() {
            self.rows.pop_front();
            self.rows.push_back(point);
        }
    }
    pub fn okay(&self) -> usize {
        self.rows.iter().filter(|point| point.okay).count()
    }
}
