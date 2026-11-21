use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct FrameMeta {
    pub frame_id: u32,
    pub timestamp_sof: u64,
    pub timestamp_eof: u64,
}

pub struct Captured<T> {
    pub metadata: FrameMeta,
    pub buffer: T,
}

pub trait CameraSource {
    type Buffer;
    type Error;
    fn receive(&mut self) -> Result<Option<Captured<Self::Buffer>>, Self::Error>;
}

pub enum CameraPair<T> {
    Single(Captured<T>),
    Dual(Captured<T>, Captured<T>),
}

impl<T> CameraPair<T> {
    pub fn main(&self) -> &Captured<T> {
        match self {
            Self::Single(main) | Self::Dual(main, _) => main,
        }
    }

    pub fn extra(&self) -> &Captured<T> {
        match self {
            Self::Single(main) => main,
            Self::Dual(_, extra) => extra,
        }
    }
}

/// One camera receive call of a pairing attempt, kept as evidence for a
/// dropped-frame record: which camera, what arrived and how long it waited.
#[derive(Clone, Debug, PartialEq)]
pub struct Received {
    pub camera: &'static str,
    pub frame_id: Option<u32>,
    pub sof: Option<u64>,
    pub wait_ms: f64,
}

fn traced<C: CameraSource>(
    source: &mut C,
    camera: &'static str,
    trace: &mut Option<&mut Vec<Received>>,
) -> Result<Option<Captured<C::Buffer>>, C::Error> {
    let Some(trace) = trace else {
        return source.receive();
    };
    let started = std::time::Instant::now();
    let frame = source.receive()?;
    trace.push(Received {
        camera,
        frame_id: frame.as_ref().map(|frame| frame.metadata.frame_id),
        sof: frame.as_ref().map(|frame| frame.metadata.timestamp_sof),
        wait_ms: started.elapsed().as_secs_f64() * 1000.0,
    });
    Ok(frame)
}

pub fn receive_pair<C: CameraSource>(
    main: &mut C,
    extra: Option<&mut C>,
) -> Result<Option<CameraPair<C::Buffer>>, C::Error> {
    receive_pair_traced(main, extra, None)
}

/// `receive_pair` that also appends every receive call to `trace`, as carrot's
/// `receive_camera_pair(..., trace=...)` does.
pub fn receive_pair_traced<C: CameraSource>(
    main: &mut C,
    extra: Option<&mut C>,
    mut trace: Option<&mut Vec<Received>>,
) -> Result<Option<CameraPair<C::Buffer>>, C::Error> {
    let Some(mut main_frame) = traced(main, "main", &mut trace)? else {
        return Ok(None);
    };
    let Some(extra) = extra else {
        return Ok(Some(CameraPair::Single(main_frame)));
    };
    let mut extra_frame = traced(extra, "extra", &mut trace)?;
    for _ in 0..10 {
        let Some(current_extra) = extra_frame.as_ref() else {
            return Ok(None);
        };
        let main_sof = main_frame.metadata.timestamp_sof;
        let extra_sof = current_extra.metadata.timestamp_sof;
        if main_sof.abs_diff(extra_sof) <= 20_000_000 {
            return Ok(extra_frame.map(|frame| CameraPair::Dual(main_frame, frame)));
        }
        if main_sof < extra_sof {
            let Some(frame) = traced(main, "main", &mut trace)? else {
                return Ok(None);
            };
            main_frame = frame;
        } else {
            extra_frame = traced(extra, "extra", &mut trace)?;
        }
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CameraStream {
    Road,
    WideRoad,
}

pub fn select_streams(road: bool, wide: bool, use_wide: bool) -> Option<(CameraStream, bool)> {
    if road {
        Some((CameraStream::Road, use_wide && wide))
    } else if use_wide && wide {
        Some((CameraStream::WideRoad, false))
    } else {
        None
    }
}
