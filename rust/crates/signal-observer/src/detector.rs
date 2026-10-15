//! Current image proposals; a housing is never an ego-lane assignment.
use crate::pixels::{Evidence, Rgb, lamp_evidence_with};
use openpilot_opencv_runtime as cv;
use openpilot_plannerd::signal::{statistics::median, tracker::overlap};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct Detection {
    #[serde(rename = "box")]
    pub bounds: [i32; 4],
    #[serde(flatten)]
    pub evidence: Evidence,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_diameter: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuity_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red_motion_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub green_rejected: Option<&'static str>,
}
impl Detection {
    pub fn new(bounds: [i32; 4], evidence: Evidence, source: &'static str) -> Self {
        Self {
            bounds,
            evidence,
            source,
            core_diameter: None,
            contrast: None,
            match_score: None,
            continuity_id: None,
            red_motion_verified: None,
            green_rejected: None,
        }
    }
    pub fn bounds_f64(&self) -> [f64; 4] {
        self.bounds.map(f64::from)
    }
}
pub(crate) fn roi(data: &[u8], width: usize, height: usize, b: [i32; 4]) -> Option<Vec<u8>> {
    let index = |v: i32, n: usize| {
        if v < 0 {
            (n as i64 + i64::from(v)).max(0) as usize
        } else {
            (v as usize).min(n)
        }
    };
    let [x1, y1, x2, y2] = [
        index(b[0], width),
        index(b[1], height),
        index(b[2], width),
        index(b[3], height),
    ];
    if x1 >= x2 || y1 >= y2 || data.len() != width.checked_mul(height)? {
        return None;
    }
    let mut out = Vec::new();
    for y in y1..y2 {
        out.extend_from_slice(&data[y * width + x1..y * width + x2]);
    }
    Some(out)
}
pub(crate) fn gray_view(
    data: &[u8],
    width: u32,
    height: u32,
) -> Result<cv::ImageView<'_>, cv::Error> {
    cv::ImageView::new(
        data,
        cv::ImageLayout::new(cv::Dimensions::new(width, height)?, cv::Format::Gray)?,
    )
}
pub fn housing_proposals(
    rgb: &Rgb,
    gray: &[u8],
    forward_only: bool,
    conservative_green: bool,
) -> Result<Vec<Detection>, cv::Error> {
    rgb.view()?;
    let (width, height) = (rgb.width as i32, rgb.height as i32);
    let y_end = (f64::from(height) * 0.60) as i32;
    if gray.len() != rgb.width as usize * rgb.height as usize {
        return Err(cv::Error::Contract("proposal grayscale extent mismatch"));
    }
    let (ox, oy, ex, ey) = if forward_only {
        (
            (f64::from(width) * 0.25) as i32,
            (f64::from(height) * 0.08) as i32,
            (f64::from(width) * 0.80) as i32,
            (f64::from(height) * 0.52) as i32,
        )
    } else {
        (0, 0, width, y_end)
    };
    let pixels = roi(gray, width as usize, height as usize, [ox, oy, ex, ey])
        .ok_or(cv::Error::Contract("housing proposal region is empty"))?;
    let dims = cv::Dimensions::new((ex - ox) as u32, (ey - oy) as u32)?;
    let mut candidates = Vec::new();
    for threshold in [45, 70, 100, 130] {
        let mask: Vec<_> = pixels
            .iter()
            .map(|v| if *v < threshold { 255 } else { 0 })
            .collect();
        let input = cv::ImageView::new(&mask, cv::ImageLayout::new(dims, cv::Format::Gray)?)?;
        let dark = cv::morphology_open(input, cv::Dimensions::new(3, 2)?)?;
        for contour in cv::external_contours(dark.view())? {
            let (x, y, w, h) = (
                contour.bounds.x + ox,
                contour.bounds.y + oy,
                contour.bounds.width as i32,
                contour.bounds.height as i32,
            );
            if forward_only && (x <= ox || y <= oy || x + w >= ex || y + h >= ey) {
                continue;
            }
            if !(16..=220).contains(&w)
                || !(4..=65).contains(&h)
                || !(2.6..=6.8).contains(&(f64::from(w) / f64::from(h)))
            {
                continue;
            }
            if f64::from(x) < f64::from(width) * 0.04
                || f64::from(x + w) > f64::from(width) * 0.96
                || y < 3
                || y + h >= y_end
            {
                continue;
            }
            let fill = contour.area / f64::from(w * h);
            if fill < 0.5 {
                continue;
            }
            let b = [x, y, x + w, y + h];
            let mut evidence = lamp_evidence_with(rgb, b, conservative_green)?;
            if evidence.raw == "unknown" {
                continue;
            }
            let interior = roi(
                &gray[..width as usize * y_end as usize],
                width as usize,
                y_end as usize,
                b,
            )
            .ok_or(cv::Error::Contract("housing sample is empty"))?;
            let mut surround = roi(
                &gray[..width as usize * y_end as usize],
                width as usize,
                y_end as usize,
                [x, (y - 3).max(0), x + w, y],
            )
            .unwrap_or_default();
            surround.extend(
                roi(
                    &gray[..width as usize * y_end as usize],
                    width as usize,
                    y_end as usize,
                    [x, y + h, x + w, y + h + 3],
                )
                .unwrap_or_default(),
            );
            let interior: Vec<_> = interior.into_iter().map(f64::from).collect();
            let surround: Vec<_> = surround.into_iter().map(f64::from).collect();
            let contrast =
                median(&surround).unwrap_or(f64::NAN) - median(&interior).unwrap_or(f64::NAN);
            if contrast < 8. {
                continue;
            }
            evidence.quality *= (fill / 0.75).min(1.);
            if evidence.quality >= 0.25 {
                candidates.push(Detection::new(b, evidence, "detected"));
            }
        }
    }
    candidates.sort_by(|a, b| b.evidence.quality.total_cmp(&a.evidence.quality));
    let mut kept: Vec<Detection> = Vec::new();
    for c in candidates {
        if kept
            .iter()
            .all(|k| overlap(c.bounds_f64(), k.bounds_f64()) < 0.35)
        {
            kept.push(c);
        }
    }
    kept.truncate(20);
    Ok(kept)
}

pub fn day_proposals(
    rgb: &Rgb,
    gray: &[u8],
    conservative_green: bool,
) -> Result<Vec<Detection>, cv::Error> {
    rgb.view()?;
    let (w, h) = (rgb.width as i32, rgb.height as i32);
    if gray.len() != rgb.width as usize * rgb.height as usize {
        return Err(cv::Error::Contract(
            "day proposal grayscale extent mismatch",
        ));
    }
    let (ox, oy, ex, ey) = (
        (f64::from(w) * 0.25) as i32,
        (f64::from(h) * 0.08) as i32,
        (f64::from(w) * 0.80) as i32,
        (f64::from(h) * 0.52) as i32,
    );
    let patch = rgb
        .crop([ox, oy, ex, ey])
        .ok_or(cv::Error::Contract("day proposal region is empty"))?;
    let hsv = cv::rgb_to_hsv(patch.view()?)?;
    let hsv = hsv.view().data();
    let width = patch.width as usize;
    let val: Vec<_> = hsv.chunks_exact(3).map(|p| p[2]).collect();
    let top = cv::morphology_tophat(
        gray_view(&val, patch.width, patch.height)?,
        cv::Dimensions::new(15, 15)?,
    )?;
    let mut red = Vec::with_capacity(val.len());
    let mut both = Vec::with_capacity(val.len());
    for (i, p) in hsv.chunks_exact(3).enumerate() {
        let bright = top.view().data()[i] >= 70;
        let r = bright && (p[0] <= 12 || p[0] >= 170) && p[1] >= 90 && p[2] >= 140;
        let g = bright
            && (38..=if conservative_green { 90 } else { 105 }).contains(&p[0])
            && p[1] >= 65
            && p[2] >= 140;
        red.push(u8::from(r) * 255);
        both.push(u8::from(r || g) * 255);
    }
    let components = cv::connected_components(gray_view(&both, patch.width, patch.height)?)?;
    let mut out = Vec::new();
    for (k, c) in components.stats.iter().enumerate().skip(1) {
        let [local_x, local_y, cw, ch] = c.bounds;
        let area = c.area;
        if !(3..=30).contains(&cw)
            || !(3..=25).contains(&ch)
            || !(5..=400).contains(&area)
            || !(0.55..=1.7).contains(&(f64::from(cw) / f64::from(ch)))
            || f64::from(area) / f64::from(cw * ch) < 0.35
        {
            continue;
        }
        let mut red_count = 0;
        for y in local_y..local_y + ch {
            for x in local_x..local_x + cw {
                let i = y as usize * width + x as usize;
                if components.labels[i] == k as i32 && red[i] != 0 {
                    red_count += 1;
                }
            }
        }
        let color = if f64::from(red_count) >= f64::from(area) * 0.9 {
            "red"
        } else if f64::from(red_count) <= f64::from(area) * 0.1 {
            "green"
        } else {
            continue;
        };
        let (x, y) = (local_x + ox, local_y + oy);
        let (cx, cy) = (c.center[0] + f64::from(ox), c.center[1] + f64::from(oy));
        let values = roi(
            &val,
            patch.width as usize,
            patch.height as usize,
            [local_x, local_y, local_x + cw, local_y + ch],
        )
        .ok_or(cv::Error::Contract("day core sample is empty"))?;
        let threshold = 95_f64.max(
            f64::from(
                *values
                    .iter()
                    .max()
                    .ok_or(cv::Error::Contract("day core sample is empty"))?,
            ) * 0.85,
        );
        let core: Vec<_> = values
            .iter()
            .map(|v| u8::from(f64::from(*v) >= threshold))
            .collect();
        let cores = cv::connected_components(gray_view(&core, cw as u32, ch as u32)?)?;
        if cores.stats.len() < 2 {
            continue;
        }
        let mut best = 1;
        for i in 2..cores.stats.len() {
            if cores.stats[i].area > cores.stats[best].area {
                best = i;
            }
        }
        let selected = &cores.stats[best];
        let (bw, bh) = (selected.bounds[2], selected.bounds[3]);
        if !(3..=18).contains(&bw)
            || !(3..=18).contains(&bh)
            || !(0.6..=1.6).contains(&(f64::from(bw) / f64::from(bh)))
            || f64::from(selected.area) / f64::from(bw * bh) < 0.55
        {
            continue;
        }
        let (ccx, ccy) = (selected.center[0], selected.center[1]);
        if (ccx + f64::from(x) - cx).abs() > f64::from(cw) * 0.25
            || (ccy + f64::from(y) - cy).abs() > f64::from(ch) * 0.25
        {
            continue;
        }
        let (cx, cy) = (ccx + f64::from(x), ccy + f64::from(y));
        let diameter = bw.max(bh);
        let (width, height) = (4 * diameter, 2 * diameter);
        let xc = if color == "red" { 0.125 } else { 0.875 };
        let b = [
            (cx - f64::from(width) * xc) as i32,
            (cy - f64::from(height) / 2.) as i32,
            (cx + f64::from(width) * (1. - xc)) as i32,
            (cy + f64::from(height) / 2.) as i32,
        ];
        let [x1, y1, x2, y2] = b;
        if x1 < 0 || y1 < 3 || x2 > w || y2 + 3 > h {
            continue;
        }
        let (ux1, ux2) = if color == "red" {
            ((cx + f64::from(diameter) * 0.75) as i32, x2)
        } else {
            (x1, (cx - f64::from(diameter) * 0.75) as i32)
        };
        let (uy1, uy2) = (
            (cy - f64::from(diameter) * 0.35) as i32,
            (cy + f64::from(diameter) * 0.35) as i32 + 1,
        );
        let Some(dark) = roi(gray, w as usize, h as usize, [ux1, uy1, ux2, uy2]) else {
            continue;
        };
        let mut surround =
            roi(gray, w as usize, h as usize, [ux1, y1 - 3, ux2, y1]).unwrap_or_default();
        surround
            .extend(roi(gray, w as usize, h as usize, [ux1, y2, ux2, y2 + 3]).unwrap_or_default());
        if surround.is_empty() {
            continue;
        }
        let dark: Vec<_> = dark.into_iter().map(f64::from).collect();
        let surround: Vec<_> = surround.into_iter().map(f64::from).collect();
        let interior = median(&dark).ok_or(cv::Error::Contract("day housing sample is empty"))?;
        let contrast = median(&surround)
            .ok_or(cv::Error::Contract("day surround sample is empty"))?
            - interior;
        if contrast < 8. || interior > 80. {
            continue;
        }
        let evidence = lamp_evidence_with(rgb, b, conservative_green)?;
        if evidence.raw == color && evidence.quality >= 0.25 {
            let mut d = Detection::new(b, evidence, "day_color_core");
            d.contrast = Some(contrast);
            out.push(d);
        }
    }
    Ok(out)
}

pub fn night_proposals(
    rgb: &Rgb,
    gray: &[u8],
    conservative_green: bool,
    is_dark: bool,
) -> Result<Vec<Detection>, cv::Error> {
    rgb.view()?;
    if !is_dark {
        return Ok(Vec::new());
    }
    let (h, w) = (rgb.height as i32, rgb.width as i32);
    let end = (f64::from(h) * 0.55) as i32;
    if gray.len() != rgb.width as usize * rgb.height as usize {
        return Err(cv::Error::Contract(
            "night proposal grayscale extent mismatch",
        ));
    }
    let (ox, oy) = (
        0.max((0.12 * f64::from(w)) as i32 - 38),
        0.max((0.08 * f64::from(h)) as i32 - 22),
    );
    let (ex, ey) = (
        w.min((0.88 * f64::from(w)).ceil() as i32 + 38),
        end.min((0.52 * f64::from(h)).ceil() as i32 + 22),
    );
    let pixels = roi(gray, w as usize, h as usize, [ox, oy, ex, ey])
        .ok_or(cv::Error::Contract("night proposal region is empty"))?;
    let mask: Vec<_> = pixels
        .iter()
        .map(|v| if *v > 180 { 255 } else { 0 })
        .collect();
    let components =
        cv::connected_components(gray_view(&mask, (ex - ox) as u32, (ey - oy) as u32)?)?;
    let mut seeds = Vec::new();
    for c in components.stats.iter().skip(1) {
        let [lx, ly, cw, ch] = c.bounds;
        let area = c.area;
        let (x, y) = (lx + ox, ly + oy);
        let (mut cx, mut cy) = (c.center[0] + f64::from(ox), c.center[1] + f64::from(oy));
        let mut diameter = cw.max(ch);
        if !(3..=38).contains(&cw)
            || !(3..=22).contains(&ch)
            || !(7..=350).contains(&area)
            || !(0.4..=3.5).contains(&(f64::from(cw) / f64::from(ch)))
            || f64::from(area) / f64::from(cw * ch) < 0.35
        {
            continue;
        }
        if !(0.12 * f64::from(w) < cx
            && cx < 0.88 * f64::from(w)
            && 0.08 * f64::from(h) < cy
            && cy < 0.52 * f64::from(h))
        {
            continue;
        }
        let halo = |cx: f64, cy: f64, diameter: i32| {
            [
                0.max((cx - 1.4 * f64::from(diameter)) as i32),
                0.max((cy - 1.4 * f64::from(diameter)) as i32),
                w.min((cx + 1.4 * f64::from(diameter)) as i32 + 1),
                end.min((cy + 1.4 * f64::from(diameter)) as i32 + 1),
            ]
        };
        let mut rect = halo(cx, cy, diameter);
        let ring_colors = |rect: [i32; 4],
                           cx: f64,
                           cy: f64,
                           diameter: i32,
                           green: bool|
         -> Result<(Vec<bool>, Vec<bool>), cv::Error> {
            let patch = rgb
                .crop(rect)
                .ok_or(cv::Error::Contract("night halo is empty"))?;
            let hsv = cv::rgb_to_hsv(patch.view()?)?;
            let hsv = hsv.view().data();
            let mut ring = Vec::new();
            let mut colors = Vec::new();
            for yy in rect[1]..rect[3] {
                for xx in rect[0]..rect[2] {
                    let i = ((yy - rect[1]) * (rect[2] - rect[0]) + xx - rect[0]) as usize;
                    let d = (f64::from(xx) - cx).powi(2) + (f64::from(yy) - cy).powi(2);
                    ring.push(
                        d >= 0.35_f64.powi(2) * f64::from(diameter * diameter)
                            && d <= 1.3_f64.powi(2) * f64::from(diameter * diameter),
                    );
                    let (hue, sat, val) = (hsv[3 * i], hsv[3 * i + 1], hsv[3 * i + 2]);
                    colors.push(if green {
                        (38..=if conservative_green { 90 } else { 105 }).contains(&hue)
                            && sat >= 65
                            && val >= 110
                    } else {
                        (hue <= 12 || hue >= 170) && sat >= 90 && val >= 110
                    });
                }
            }
            Ok((ring, colors))
        };
        let (mut ring, mut colors) = ring_colors(rect, cx, cy, diameter, false)?;
        let enough = |ring: &[bool], colors: &[bool]| {
            ring.iter().zip(colors).filter(|(r, c)| **r && **c).count() as f64
                >= 8_f64.max(ring.iter().filter(|v| **v).count() as f64 * 0.22)
        };
        let is_red = enough(&ring, &colors);
        let (state, xc) = if is_red {
            if !(0.65..=1.55).contains(&(f64::from(cw) / f64::from(ch)))
                || f64::from(area) / f64::from(cw * ch) < 0.55
            {
                continue;
            }
            let mut counts = [0_usize; 4];
            let mut colored = [0_usize; 4];
            for yy in rect[1]..rect[3] {
                for xx in rect[0]..rect[2] {
                    let i = ((yy - rect[1]) * (rect[2] - rect[0]) + xx - rect[0]) as usize;
                    if ring[i] {
                        let q =
                            usize::from(f64::from(xx) >= cx) + 2 * usize::from(f64::from(yy) >= cy);
                        counts[q] += 1;
                        colored[q] += usize::from(colors[i]);
                    }
                }
            }
            let coverage: [f64; 4] = std::array::from_fn(|i| colored[i] as f64 / counts[i] as f64);
            let low = coverage[1..].iter().fold(coverage[0], |a, b| {
                openpilot_control_policy::math::minimum(a, *b)
            });
            let high = coverage[1..].iter().fold(coverage[0], |a, b| {
                openpilot_control_policy::math::maximum(a, *b)
            });
            if low < 0.4 || high - low > 0.45 {
                continue;
            }
            ("red", 0.125)
        } else {
            let mut core = vec![0; ((cw + 2) * (ch + 2)) as usize];
            for yy in 0..ch {
                for xx in 0..cw {
                    core[((yy + 1) * (cw + 2) + xx + 1) as usize] =
                        u8::from(gray[((y + yy) * w + x + xx) as usize] > 180);
                }
            }
            let distances =
                cv::distance_transform(gray_view(&core, (cw + 2) as u32, (ch + 2) as u32)?)?;
            let mut best = 0;
            for i in 1..distances.len() {
                if distances[i] > distances[best] {
                    best = i;
                }
            }
            let radius = f64::from(distances[best]);
            if radius < 1.5 {
                continue;
            }
            cx = f64::from(x + (best % (cw + 2) as usize) as i32 - 1);
            cy = f64::from(y + (best / (cw + 2) as usize) as i32 - 1);
            diameter = 3.max((2. * radius).round_ties_even() as i32);
            rect = halo(cx, cy, diameter);
            (ring, colors) = ring_colors(rect, cx, cy, diameter, true)?;
            if !enough(&ring, &colors) {
                continue;
            }
            ("green", 0.875)
        };
        if state == "red" {
            let mut outside = 0;
            for yy in rect[1]..rect[3] {
                for xx in rect[0]..rect[2] {
                    if gray[(yy * w + xx) as usize] > 180
                        && (f64::from(xx) - cx).powi(2) + (f64::from(yy) - cy).powi(2)
                            > f64::from(diameter * diameter)
                    {
                        outside += 1;
                    }
                }
            }
            if f64::from(outside) > f64::from(area) * 0.3 {
                continue;
            }
        }
        let (bw, bh) = (4 * diameter, 6.max(2 * diameter));
        let b = [
            (cx - f64::from(bw) * xc) as i32,
            (cy - f64::from(bh) / 2.) as i32,
            (cx + f64::from(bw) * (1. - xc)) as i32,
            (cy + f64::from(bh) / 2.) as i32,
        ];
        if b[0] < 0 || b[1] < 0 || b[2] >= w || b[3] >= end {
            continue;
        }
        let mut evidence = lamp_evidence_with(rgb, b, conservative_green)?;
        if evidence.raw != state {
            continue;
        }
        evidence.quality = evidence.quality.max(0.25);
        let mut d = Detection::new(b, evidence, "night_lamp_core");
        d.core_diameter = Some(diameter);
        seeds.push(d);
    }
    Ok(seeds)
}

pub fn detect(
    rgb: &Rgb,
    gray: &[u8],
    daytime_cores: bool,
    conservative_green: bool,
) -> Result<Vec<Detection>, cv::Error> {
    rgb.view()?;
    if gray.len() != rgb.width as usize * rgb.height as usize {
        return Err(cv::Error::Contract("detector grayscale extent mismatch"));
    }
    let end = (f64::from(rgb.height) * 0.55) as usize;
    let dark = crate::pixels::histogram_dark(&gray[..end * rgb.width as usize])?;
    let mut candidates = housing_proposals(rgb, gray, daytime_cores && !dark, conservative_green)?;
    let extra = if daytime_cores && !dark {
        day_proposals(rgb, gray, conservative_green)?
    } else {
        night_proposals(rgb, gray, conservative_green, dark)?
    };
    for c in extra {
        if candidates
            .iter()
            .all(|other| overlap(c.bounds_f64(), other.bounds_f64()) < 0.35)
        {
            candidates.push(c);
        }
    }
    candidates.truncate(20);
    Ok(candidates)
}
