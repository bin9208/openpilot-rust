use crate::models::DebugPlotSnapshot;
use crate::renderer::tokens::{DEBUG_PLOT_MAX_SAMPLES, DEBUG_PLOT_SAMPLE_SECONDS};

pub struct PlotHistory {
    mode: i64,
    count: usize,
    index: usize,
    values: [[f64; DEBUG_PLOT_MAX_SAMPLES]; 3],
    minimum: f64,
    maximum: f64,
    sampled: Option<f64>,
}
impl Default for PlotHistory {
    fn default() -> Self {
        Self {
            mode: -1,
            count: 0,
            index: DEBUG_PLOT_MAX_SAMPLES - 1,
            values: [[0.0; DEBUG_PLOT_MAX_SAMPLES]; 3],
            minimum: -2.0,
            maximum: 2.0,
            sampled: None,
        }
    }
}
impl PlotHistory {
    pub fn update(&mut self, plot: Option<&DebugPlotSnapshot>, now: f64) {
        let Some(plot) = plot.filter(|plot| plot.mode > 0) else {
            if self.mode != 0 {
                self.clear(0);
            }
            return;
        };
        if self.mode != plot.mode {
            self.clear(plot.mode);
        }
        if self
            .sampled
            .is_none_or(|sampled| now - sampled >= DEBUG_PLOT_SAMPLE_SECONDS)
        {
            self.append(plot.values);
            self.sampled = Some(now);
        }
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    #[must_use]
    pub fn bounds(&self) -> (f64, f64) {
        (self.minimum, self.maximum)
    }
    #[must_use]
    pub fn samples(&self) -> impl ExactSizeIterator<Item = [f64; 3]> + '_ {
        let oldest =
            (self.index + DEBUG_PLOT_MAX_SAMPLES + 1 - self.count) % DEBUG_PLOT_MAX_SAMPLES;
        (0..self.count).map(move |offset| {
            let index = (oldest + offset) % DEBUG_PLOT_MAX_SAMPLES;
            [
                self.values[0][index],
                self.values[1][index],
                self.values[2][index],
            ]
        })
    }
    pub(super) fn sample_at(&self, offset: usize) -> [f64; 3] {
        let oldest =
            (self.index + DEBUG_PLOT_MAX_SAMPLES + 1 - self.count) % DEBUG_PLOT_MAX_SAMPLES;
        let index = (oldest + offset) % DEBUG_PLOT_MAX_SAMPLES;
        [
            self.values[0][index],
            self.values[1][index],
            self.values[2][index],
        ]
    }
    fn clear(&mut self, mode: i64) {
        *self = Self {
            mode,
            ..Self::default()
        };
    }
    fn append(&mut self, values: [f64; 3]) {
        self.index = (self.index + 1) % DEBUG_PLOT_MAX_SAMPLES;
        self.count = (self.count + 1).min(DEBUG_PLOT_MAX_SAMPLES);
        for (series, value) in self.values.iter_mut().zip(values) {
            series[self.index] = if value.is_finite() { value } else { 0.0 };
        }
        let (mut minimum, mut maximum) = (f64::INFINITY, f64::NEG_INFINITY);
        for sample in self.samples() {
            for value in sample {
                minimum = minimum.min(value);
                maximum = maximum.max(value);
            }
        }
        if minimum == f64::INFINITY || maximum == f64::NEG_INFINITY {
            minimum = -2.0;
            maximum = 2.0;
        }
        minimum = minimum.min(-2.0);
        maximum = maximum.max(2.0);
        if maximum - minimum < 0.001 {
            minimum -= 1.0;
            maximum += 1.0;
        }
        self.minimum = minimum;
        self.maximum = maximum;
    }
}
