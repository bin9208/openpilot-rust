use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    os::unix::fs::FileExt,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub name: String,
    pub shape: Vec<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub model_sha256: String,
    pub checkpoint: String,
    pub inputs: Vec<Input>,
    pub output_count: usize,
    pub output_slices: BTreeMap<String, [Option<usize>; 3]>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub offset: usize,
    pub shape: Vec<usize>,
    pub dtype: String,
}
impl View {
    fn bytes(&self) -> Result<usize, Error> {
        self.shape
            .iter()
            .try_fold(if self.dtype == "uint8" { 1usize } else { 4 }, |n, d| {
                n.checked_mul(*d)
                    .ok_or(Error::Contract("worker shape overflow"))
            })
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    pub size: usize,
    pub input_bytes: usize,
    pub output_count: usize,
    pub layout: BTreeMap<String, View>,
    pub input_shapes: BTreeMap<String, Vec<usize>>,
    pub output_slices: BTreeMap<String, [Option<usize>; 3]>,
    pub checkpoint: String,
    pub frame_size: usize,
}
impl Info {
    pub fn new(metadata: Metadata, camera: [u32; 2]) -> Result<Self, Error> {
        if ![[1344, 760], [1928, 1208]].contains(&camera)
            || metadata.checkpoint.is_empty()
            || metadata.output_count == 0
            || metadata.output_count > 1_000_000
            || metadata.output_slices.is_empty()
            || metadata.output_slices.len() > 256
            || metadata.inputs.len() != 3
        {
            return Err(Error::Contract("unsupported worker metadata"));
        }
        for section in metadata.output_slices.values() {
            if !matches!(section, [Some(start), Some(stop), None | Some(1)] if start < stop && *stop <= metadata.output_count)
            {
                return Err(Error::Contract("invalid worker output slice"));
            }
        }
        let [width, height] = camera;
        let frame_size = (width.next_multiple_of(128)
            * (height.next_multiple_of(32) + (height / 2).next_multiple_of(16)))
            as usize;
        let mut layout = BTreeMap::from([
            (
                "tfm".into(),
                View {
                    offset: 0,
                    shape: vec![3, 3],
                    dtype: "float32".into(),
                },
            ),
            (
                "big_tfm".into(),
                View {
                    offset: 36,
                    shape: vec![3, 3],
                    dtype: "float32".into(),
                },
            ),
        ]);
        let mut offset = 128;
        for input in metadata.inputs {
            let size: usize = match input.name.as_str() {
                "desire" if input.shape == [8] => 32,
                "traffic_convention" | "action_t" if input.shape == [1, 2] => 8,
                _ => return Err(Error::Contract("unsupported worker input shape")),
            };
            if layout
                .insert(
                    input.name,
                    View {
                        offset,
                        shape: input.shape,
                        dtype: "float32".into(),
                    },
                )
                .is_some()
            {
                return Err(Error::Contract("duplicate worker input"));
            }
            offset += size.next_multiple_of(128);
        }
        let mut input_shapes = BTreeMap::new();
        for name in ["img", "big_img"] {
            layout.insert(
                name.into(),
                View {
                    offset,
                    shape: vec![frame_size],
                    dtype: "uint8".into(),
                },
            );
            input_shapes.insert(name.into(), vec![frame_size]);
            offset += frame_size;
        }
        Ok(Self {
            size: offset + metadata.output_count * 4,
            input_bytes: offset,
            output_count: metadata.output_count,
            layout,
            input_shapes,
            output_slices: metadata.output_slices,
            checkpoint: metadata.checkpoint,
            frame_size,
        })
    }
    pub fn input<'a>(&self, bytes: &'a [u8], name: &str) -> Result<&'a [u8], Error> {
        let view = self
            .layout
            .get(name)
            .ok_or(Error::Contract("worker view missing"))?;
        bytes
            .get(view.offset..view.offset + view.bytes()?)
            .ok_or(Error::Contract("worker input outside shared buffer"))
    }
}
pub trait Runtime {
    fn run(&mut self, packed: &[u8], info: &Info, output: &mut [u8]) -> Result<(), Error>;
}
pub fn watch_parent() -> bool {
    let parent = rustix::process::getppid();
    if parent.is_none_or(|pid| pid.as_raw_nonzero().get() == 1) {
        return false;
    }
    std::thread::spawn(move || {
        while rustix::process::getppid() == parent {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        std::process::exit(1);
    });
    true
}
pub fn serve(
    runtime: &mut impl Runtime,
    file: &File,
    info: &Info,
    control_in: &mut impl Read,
    control_out: &mut impl Write,
) -> Result<(), Error> {
    file.set_len(info.size as u64)?;
    let mut packed = vec![0; info.input_bytes];
    let mut output = vec![0; info.output_count * 4];
    file.write_all_at(&packed, 0)?;
    serde_json::to_writer(&mut *control_out, info)?;
    control_out.write_all(b"\n")?;
    control_out.flush()?;
    let mut command = [0];
    while control_in.read(&mut command)? != 0 {
        match command[0] {
            b'q' => break,
            b'r' => {
                file.read_exact_at(&mut packed, 0)?;
                runtime.run(&packed, info, &mut output)?;
                if output
                    .chunks_exact(4)
                    .any(|value| !f32::from_le_bytes(value.try_into().unwrap()).is_finite())
                {
                    return Err(Error::Contract("invalid precompiled model output"));
                }
                file.write_all_at(&output, info.input_bytes as u64)?;
                control_out.write_all(b"1\n")?;
                control_out.flush()?;
            }
            _ => return Err(Error::Contract("invalid model worker command")),
        }
    }
    Ok(())
}
pub fn report_error(error: &Error, control: &mut impl Write) -> std::io::Result<()> {
    let text = error.to_string();
    let mut start = text.len().saturating_sub(16384);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    control.write_all(b"ERROR ")?;
    serde_json::to_writer(&mut *control, &text[start..])?;
    control.write_all(b"\n")?;
    control.flush()
}
