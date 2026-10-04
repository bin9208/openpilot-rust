use openpilot_ui_framework::{geometry::Rect, text_layout::float};

#[derive(Clone, Copy, Default)]
pub enum Transform {
    #[default]
    Fit,
    DriverLarge,
    DriverCompact,
    Matrix([[f64; 3]; 3]),
}
impl Transform {
    pub fn matrix(self, rect: Rect, frame: Option<(i32, i32)>) -> [[f64; 3]; 3] {
        let width = frame.map_or(1928.0, |f| f64::from(f.0));
        let height = frame.map_or(1208.0, |f| f64::from(f.1));
        let (zx, zy) = match self {
            Self::Matrix(matrix) => return matrix,
            Self::DriverLarge => {
                let y = height * 2.0 / width;
                (
                    y * f64::from(rect.height) / f64::from(rect.width) * width / height,
                    y,
                )
            }
            Self::Fit | Self::DriverCompact => {
                if frame.is_none() {
                    return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
                }
                let widget = f64::from(rect.width) / f64::from(rect.height);
                let camera = width / height;
                let scale = if matches!(self, Self::DriverCompact) {
                    1.5
                } else {
                    1.0
                };
                (
                    (camera / widget).min(1.0) * scale,
                    (widget / camera).min(1.0) * scale,
                )
            }
        };
        [[zx, 0.0, 0.0], [0.0, zy, 0.0], [0.0, 0.0, 1.0]]
    }
    pub fn destination(self, rect: Rect, frame: (i32, i32)) -> Rect {
        let matrix = self.matrix(rect, Some(frame));
        let width = f64::from(rect.width) * matrix[0][0];
        let height = f64::from(rect.height) * matrix[1][1];
        Rect {
            x: float(
                f64::from(rect.x)
                    + (f64::from(rect.width) - width) / 2.0
                    + matrix[0][2] * f64::from(rect.width) / 2.0,
            ),
            y: float(
                f64::from(rect.y)
                    + (f64::from(rect.height) - height) / 2.0
                    + matrix[1][2] * f64::from(rect.height) / 2.0,
            ),
            width: float(width),
            height: float(height),
        }
    }
}
