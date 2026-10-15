use openpilot_opencv_runtime as cv;
use openpilot_plannerd::signal::statistics::{median, percentile};
use serde::{Deserialize, Serialize};
#[derive(Debug, Deserialize)]
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}
impl Rgb {
    pub fn view(&self) -> Result<cv::ImageView<'_>, cv::Error> {
        cv::ImageView::new(
            &self.data,
            cv::ImageLayout::new(
                cv::Dimensions::new(self.width, self.height)?,
                cv::Format::Rgb,
            )?,
        )
    }
    pub fn crop(&self, b: [i32; 4]) -> Option<Self> {
        self.view().ok()?;
        let index = |v: i32, n: u32| {
            if v < 0 {
                (i64::from(n) + i64::from(v)).max(0) as u32
            } else {
                (v as u32).min(n)
            }
        };
        let [x1, y1, x2, y2] = [
            index(b[0], self.width),
            index(b[1], self.height),
            index(b[2], self.width),
            index(b[3], self.height),
        ];
        if x1 >= x2 || y1 >= y2 {
            return None;
        }
        let mut data = Vec::new();
        let stride = self.width as usize * 3;
        for y in y1..y2 {
            data.extend_from_slice(
                &self.data
                    [y as usize * stride + x1 as usize * 3..y as usize * stride + x2 as usize * 3],
            );
        }
        Some(Self {
            width: x2 - x1,
            height: y2 - y1,
            data,
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Evidence {
    pub raw: &'static str,
    pub quality: f64,
    pub red_score: f64,
    pub green_score: f64,
    pub left_brightness: f64,
    pub right_brightness: f64,
}
impl Default for Evidence {
    fn default() -> Self {
        Self {
            raw: "unknown",
            quality: 0.,
            red_score: 0.,
            green_score: 0.,
            left_brightness: 0.,
            right_brightness: 0.,
        }
    }
}
pub fn dark_scene(rgb: &Rgb) -> Result<bool, cv::Error> {
    rgb.view()?;
    let end = (f64::from(rgb.height) * 0.55) as u32;
    if end == 0 {
        return Err(cv::Error::Contract("dark-scene sample has no rows"));
    }
    let crop = rgb
        .crop([0, 0, rgb.width as i32, end as i32])
        .ok_or(cv::Error::Contract("dark-scene sample is empty"))?;
    let gray = cv::rgb_to_gray(crop.view()?)?;
    histogram_dark(gray.view().data())
}
pub(crate) fn histogram_dark(pixels: &[u8]) -> Result<bool, cv::Error> {
    if pixels.is_empty() {
        return Err(cv::Error::Contract("dark-scene sample is empty"));
    }
    let mut histogram = [0_usize; 256];
    for &p in pixels {
        histogram[p as usize] += 1;
    }
    let rank = 0.7 * (pixels.len() - 1) as f64;
    let mut cumulative = 0;
    let mut lower = 0;
    let mut upper = 0;
    for (value, count) in histogram.iter().enumerate() {
        cumulative += count;
        if cumulative <= rank.floor() as usize {
            lower = value + 1;
        }
        if cumulative <= rank.ceil() as usize {
            upper = value + 1;
        }
    }
    Ok(lower as f64 + (upper - lower) as f64 * (rank - rank.floor()) <= 45.)
}
pub fn lamp_evidence(rgb: &Rgb, b: [i32; 4]) -> Result<Evidence, cv::Error> {
    lamp_evidence_with(rgb, b, false)
}
pub fn lamp_evidence_with(
    rgb: &Rgb,
    b: [i32; 4],
    conservative_green: bool,
) -> Result<Evidence, cv::Error> {
    rgb.view()?;
    let Some(patch) = rgb.crop(b) else {
        return Ok(Evidence::default());
    };
    let gray = cv::rgb_to_gray(patch.view()?)?;
    let hsv = cv::rgb_to_hsv(patch.view()?)?;
    let gray: Vec<_> = gray
        .view()
        .data()
        .iter()
        .map(|v| f64::from(*v) / 255.)
        .collect();
    let hsv = hsv.view().data();
    let (w, h) = (patch.width as usize, patch.height as usize);
    let background = median(&gray).ok_or(cv::Error::Contract("lamp patch has no pixels"))?;
    let mut scores = [0_f64; 2];
    let mut peaks = [0.; 2];
    for (which, xc) in [0.125, 0.875].into_iter().enumerate() {
        let mut mask = vec![0; w * h];
        let mut disk = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let (hue, sat, val) = (hsv[3 * i], hsv[3 * i + 1], hsv[3 * i + 2]);
                let color = if which == 0 {
                    (hue <= 12 || hue >= 170) && sat >= 90 && val >= 95
                } else {
                    (38..=if conservative_green { 90 } else { 105 }).contains(&hue)
                        && sat >= 65
                        && val >= 95
                };
                mask[i] = u8::from(
                    color
                        && x as f64 >= (xc - 0.13) * w as f64
                        && x as f64 <= (xc + 0.13) * w as f64
                        && y as f64 >= 0.15 * h as f64
                        && y as f64 <= 0.85 * h as f64,
                );
                if (x as f64 + 0.5 - xc * w as f64).powi(2)
                    + (y as f64 + 0.5 - 0.5 * h as f64).powi(2)
                    <= 1_f64.max(0.32 * h as f64).powi(2)
                {
                    disk.push(gray[i]);
                }
            }
        }
        let dims = cv::Dimensions::new(patch.width, patch.height)?;
        let input = cv::ImageView::new(&mask, cv::ImageLayout::new(dims, cv::Format::Gray)?)?;
        let components = cv::connected_components(input)?;
        for (index, c) in components.stats.iter().enumerate().skip(1) {
            let (_, _, width, height) = (c.bounds[0], c.bounds[1], c.bounds[2], c.bounds[3]);
            if c.area < 2 || !(0.4..=2.5).contains(&(f64::from(width) / f64::from(height.max(1)))) {
                continue;
            }
            let position = (1. - (c.center[0] / w as f64 - xc).abs() / 0.22).max(0.)
                * (1. - (c.center[1] / h as f64 - 0.5).abs() / 0.6).max(0.);
            let values: Vec<_> = components
                .labels
                .iter()
                .enumerate()
                .filter(|(_, label)| **label == index as i32)
                .map(|(i, _)| f64::from(hsv[3 * i + 2]))
                .collect();
            let strength = percentile(&values, 80.)
                .ok_or(cv::Error::Contract("component has no pixels"))?
                / 255.;
            let score = position
                * strength
                * (f64::from(c.area) / (2_f64.max((h * h) as f64 * 0.12))).min(1.);
            scores[which] = scores[which].max(score);
        }
        peaks[which] = (percentile(&disk, 85.)
            .ok_or(cv::Error::Contract("lamp disk has no pixels"))?
            - background)
            .max(0.);
    }
    let [red, green] = scores;
    Ok(Evidence {
        raw: if red >= 0.18 && green < 0.12 {
            "red"
        } else if green >= 0.18 && red < 0.12 {
            "green"
        } else {
            "unknown"
        },
        quality: red.max(green),
        red_score: red,
        green_score: green,
        left_brightness: peaks[0],
        right_brightness: peaks[1],
    })
}

pub fn illuminated_green(rgb: &Rgb, b: [i32; 4]) -> Result<bool, cv::Error> {
    rgb.view()?;
    let Some(patch) = rgb.crop(b) else {
        return Ok(false);
    };
    let (w, h) = (patch.width as usize, patch.height as usize);
    let mut mask = vec![0; w * h];
    let mut housing_g = Vec::new();
    let mut housing_diff = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let (r, g, b) = (
                f64::from(patch.data[3 * i]),
                f64::from(patch.data[3 * i + 1]),
                f64::from(patch.data[3 * i + 2]),
            );
            let row = y as f64 >= 0.18 * h as f64 && (y as f64) < 0.82 * h as f64;
            mask[i] = u8::from(
                x as f64 >= 0.70 * w as f64
                    && (x as f64) < 0.97 * w as f64
                    && row
                    && g >= 70.
                    && g - r >= 30.
                    && g >= 0.9 * b
                    && (g >= 0.98 * b || g - r >= 60.),
            );
            if x as f64 >= 0.35 * w as f64 && (x as f64) < 0.70 * w as f64 && row {
                housing_g.push(g);
                housing_diff.push(g - r);
            }
        }
    }
    let dims = cv::Dimensions::new(patch.width, patch.height)?;
    let input = cv::ImageView::new(&mask, cv::ImageLayout::new(dims, cv::Format::Gray)?)?;
    let components = cv::connected_components(input)?;
    if housing_g.is_empty() {
        return Ok(false);
    }
    let base_g = median(&housing_g).ok_or(cv::Error::Contract("housing has no samples"))?;
    let base_diff = median(&housing_diff).ok_or(cv::Error::Contract("housing has no samples"))?;
    for (index, c) in components.stats.iter().enumerate().skip(1) {
        if c.area < 2
            || !(0.4..=2.5).contains(&(f64::from(c.bounds[2]) / f64::from(c.bounds[3].max(1))))
        {
            continue;
        }
        let mut green = Vec::new();
        let mut difference = Vec::new();
        for (i, label) in components.labels.iter().enumerate() {
            if *label == index as i32 {
                let g = f64::from(patch.data[3 * i + 1]);
                green.push(g);
                difference.push(g - f64::from(patch.data[3 * i]));
            }
        }
        if percentile(&green, 80.).is_some_and(|v| v - base_g >= 18.)
            && percentile(&difference, 80.).is_some_and(|v| v - base_diff >= 12.)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
