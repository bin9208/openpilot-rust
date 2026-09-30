use crate::FFT_SAMPLES;
use rustfft::{num_complex::Complex, num_traits::ToPrimitive, Fft, FftPlanner};
use std::sync::Arc;

const FFT_LENGTH: f64 = 1600.;
const REFERENCE_SPL: f64 = 2e-5;

#[derive(Clone, Copy, Debug, Default)]
pub struct Pressure {
    pub unweighted: f64,
    pub weighted: f64,
    pub weighted_db: f64,
}

pub struct Analyzer {
    pending: [f64; FFT_SAMPLES],
    pending_len: usize,
    window: [f64; FFT_SAMPLES],
    filter: [f64; FFT_SAMPLES],
    transform: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
    forward: Arc<dyn Fft<f64>>,
    inverse: Arc<dyn Fft<f64>>,
    pressure: Pressure,
}

impl Default for Analyzer {
    fn default() -> Self {
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(FFT_SAMPLES);
        let inverse = planner.plan_fft_inverse(FFT_SAMPLES);
        let scratch_len = forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len());
        let mut filter = [0.; FFT_SAMPLES];
        let mut window = [0.; FFT_SAMPLES];
        for ((index, filter), window) in (0..1600_i32).zip(&mut filter).zip(&mut window) {
            let frequency = f64::from(if index < 800 { index } else { index - 1600 }) * 10.;
            let squared = frequency * frequency;
            *filter = 12194_f64.powi(2) * squared * squared
                / ((squared + 20.6_f64.powi(2))
                    * (squared + 12194_f64.powi(2))
                    * ((squared + 107.7_f64.powi(2)) * (squared + 737.9_f64.powi(2))).sqrt());
            *window =
                0.5 + 0.5 * (std::f64::consts::PI * f64::from(2 * index - 1599) / 1599.).cos();
        }
        let maximum = filter.iter().copied().fold(0., f64::max);
        filter.iter_mut().for_each(|value| *value /= maximum);
        Self {
            pending: [0.; FFT_SAMPLES],
            pending_len: 0,
            window,
            filter,
            transform: vec![Complex::default(); FFT_SAMPLES],
            scratch: vec![Complex::default(); scratch_len],
            forward,
            inverse,
            pressure: Pressure::default(),
        }
    }
}

impl Analyzer {
    pub fn pressure(&self) -> Pressure {
        self.pressure
    }
    pub fn pending_len(&self) -> usize {
        self.pending_len
    }
    pub fn append(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.pending[self.pending_len] = f64::from(sample);
            self.pending_len += 1;
            if self.pending_len == FFT_SAMPLES {
                self.analyze();
                self.pending_len = 0;
            }
        }
    }
    fn analyze(&mut self) {
        let unweighted =
            (self.pending.iter().map(|value| value * value).sum::<f64>() / FFT_LENGTH).sqrt();
        for ((slot, &value), &window) in self
            .transform
            .iter_mut()
            .zip(&self.pending)
            .zip(&self.window)
        {
            *slot = Complex::new(value * window, 0.);
        }
        self.forward
            .process_with_scratch(&mut self.transform, &mut self.scratch);
        for (slot, &filter) in self.transform.iter_mut().zip(&self.filter) {
            *slot *= filter;
        }
        self.inverse
            .process_with_scratch(&mut self.transform, &mut self.scratch);
        let weighted = (self
            .transform
            .iter()
            .map(|value| (value.norm() / FFT_LENGTH).powi(2))
            .sum::<f64>()
            / FFT_LENGTH)
            .sqrt();
        let weighted_db = if weighted > 0. {
            20. * (weighted / REFERENCE_SPL).log10()
        } else {
            0.
        };
        self.pressure = Pressure {
            unweighted,
            weighted,
            weighted_db,
        };
    }
}

pub fn raw_audio(samples: &[f32], bytes: &mut Vec<u8>) {
    bytes.clear();
    for sample in samples {
        let value = (sample * 32767.).to_i32().unwrap_or(i32::MIN);
        let native = value.to_ne_bytes();
        let low = if cfg!(target_endian = "little") {
            [native[0], native[1]]
        } else {
            [native[2], native[3]]
        };
        bytes.extend_from_slice(&low);
    }
}
