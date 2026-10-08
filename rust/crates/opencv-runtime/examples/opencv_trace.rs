use openpilot_opencv_runtime::{
    self as cv, Dimensions, DnnNet, Format, ImageLayout, ImageView, Point, TensorView,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Pixels {
    Gray,
    Rgb,
    Bgr,
}
impl From<Pixels> for Format {
    fn from(value: Pixels) -> Self {
        match value {
            Pixels::Gray => Self::Gray,
            Pixels::Rgb => Self::Rgb,
            Pixels::Bgr => Self::Bgr,
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Resize {
        input: PathBuf,
        width: u32,
        height: u32,
        format: Pixels,
        output_width: u32,
        output_height: u32,
    },
    BgrGray {
        input: PathBuf,
        width: u32,
        height: u32,
    },
    Nv12Rgb {
        input: PathBuf,
        width: u32,
        height: u32,
    },
    PolygonMask {
        width: u32,
        height: u32,
        points: Vec<[i32; 2]>,
    },
    Bounds {
        points: Vec<[i32; 2]>,
    },
    MaskImage {
        input: PathBuf,
        mask: PathBuf,
        width: u32,
        height: u32,
        format: Pixels,
    },
    Dnn {
        model: PathBuf,
        input: PathBuf,
        shape: Vec<i32>,
        names: Vec<String>,
    },
}
#[derive(Serialize)]
struct OutputTensor {
    shape: Vec<i32>,
    file: PathBuf,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Response {
    Image {
        width: u32,
        height: u32,
        bytes: usize,
        file: PathBuf,
    },
    Bounds {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    Dnn {
        available_names: Vec<String>,
        outputs: Vec<OutputTensor>,
    },
}

fn image_output(value: cv::Image, output: &Path) -> Result<Response, Box<dyn std::error::Error>> {
    let dimensions = value.view().layout().dimensions();
    let bytes = value.view().data().len();
    let file = output.with_extension("u8");
    fs::write(&file, value.into_data())?;
    Ok(Response::Image {
        width: dimensions.width(),
        height: dimensions.height(),
        bytes,
        file,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("opencv_trace INPUT.json OUTPUT.json".into());
    }
    let request: Request = serde_json::from_slice(&fs::read(&args[0])?)?;
    let output = PathBuf::from(&args[1]);
    cv::initialize(2)?;
    let response = match request {
        Request::Dnn {
            model,
            input,
            shape,
            names,
        } => {
            let bytes = fs::read(input)?;
            if bytes.len() % 4 != 0 {
                return Err("f32 input must contain complete little-endian values".into());
            }
            let values: Vec<_> = bytes
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]))
                .collect();
            let mut net = DnnNet::load(&model)?;
            let available_names = net.output_names()?;
            let tensors = net.forward(TensorView::new(&shape, &values)?, &names)?;
            let mut outputs = Vec::new();
            for (index, tensor) in tensors.into_iter().enumerate() {
                let (shape, values) = tensor.into_parts();
                let file = output.with_extension(format!("{index}.f32"));
                let bytes: Vec<_> = values
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect();
                fs::write(&file, bytes)?;
                outputs.push(OutputTensor { shape, file });
            }
            Response::Dnn {
                available_names,
                outputs,
            }
        }
        Request::Bounds { points } => {
            let polygon: Vec<_> = points.iter().map(|p| Point { x: p[0], y: p[1] }).collect();
            let rect = cv::bounding_rect(&polygon)?;
            Response::Bounds {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            }
        }
        Request::Resize {
            input,
            width,
            height,
            format,
            output_width,
            output_height,
        } => {
            let bytes = fs::read(input)?;
            let layout = ImageLayout::new(Dimensions::new(width, height)?, format.into())?;
            image_output(
                cv::resize_linear(
                    ImageView::new(&bytes, layout)?,
                    Dimensions::new(output_width, output_height)?,
                )?,
                &output,
            )?
        }
        Request::BgrGray {
            input,
            width,
            height,
        } => {
            let bytes = fs::read(input)?;
            let layout = ImageLayout::new(Dimensions::new(width, height)?, Format::Bgr)?;
            image_output(cv::bgr_to_gray(ImageView::new(&bytes, layout)?)?, &output)?
        }
        Request::Nv12Rgb {
            input,
            width,
            height,
        } => image_output(
            cv::nv12_to_rgb(&fs::read(input)?, Dimensions::new(width, height)?)?,
            &output,
        )?,
        Request::PolygonMask {
            width,
            height,
            points,
        } => {
            let polygon: Vec<_> = points.iter().map(|p| Point { x: p[0], y: p[1] }).collect();
            image_output(
                cv::polygon_mask(Dimensions::new(width, height)?, &polygon)?,
                &output,
            )?
        }
        Request::MaskImage {
            input,
            mask,
            width,
            height,
            format,
        } => {
            let bytes = fs::read(input)?;
            let mask = fs::read(mask)?;
            let dimensions = Dimensions::new(width, height)?;
            image_output(
                cv::apply_mask(
                    ImageView::new(&bytes, ImageLayout::new(dimensions, format.into())?)?,
                    ImageView::new(&mask, ImageLayout::new(dimensions, Format::Gray)?)?,
                )?,
                &output,
            )?
        }
    };
    fs::write(
        output.with_extension("maps.txt"),
        fs::read("/proc/self/maps")?,
    )?;
    fs::write(output, serde_json::to_vec(&response)?)?;
    Ok(())
}
