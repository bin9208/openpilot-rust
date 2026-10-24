use crate::{driver::driver_transform, Error};
use openpilot_model_runtime::{
    catalog::{Bundle, Kind},
    qcom::QcomModel,
    CpuModel,
};
use openpilot_modeld::{
    driver_wire::{self, DriverTiming},
    parse::RawOutputs,
    prediction::DriverPrediction,
};
use openpilot_msgq::{VisionFrame, VisionMetadata};
use std::time::Instant;

enum Backend {
    Cpu(CpuModel),
    Qcom(QcomModel),
}

impl Backend {
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        match self {
            Self::Cpu(model) => model.write_input(name, bytes)?,
            Self::Qcom(model) => model.write_input(name, bytes)?,
        }
        Ok(())
    }

    fn execute(&mut self, name: &str) -> Result<(), Error> {
        match self {
            Self::Cpu(model) => model.run_entry(name)?,
            Self::Qcom(model) => model.run_entry(name)?,
        }
        Ok(())
    }

    fn read(&self, destination: &mut [u8]) -> Result<(), Error> {
        match self {
            Self::Cpu(model) => model.read_output("model", destination)?,
            Self::Qcom(model) => {
                let source = model.read_output("model")?;
                if source.len() != destination.len() {
                    return Err(Error::Contract("driver output size changed"));
                }
                destination.copy_from_slice(source);
            }
        }
        Ok(())
    }
}

pub struct DriverRuntime<'a> {
    bundle: &'a Bundle,
    model: Backend,
    frame_bytes: Vec<u8>,
    output_bytes: Vec<u8>,
    output_values: Vec<f32>,
    transform: Vec<u8>,
}

impl<'a> DriverRuntime<'a> {
    /// # Safety
    /// The catalog must contain trusted, immutable native kernels satisfying
    /// the CPU/QCOM model load contracts for the entire runtime lifetime.
    pub unsafe fn load(bundle: &'a Bundle, priority: u8) -> Result<Self, Error> {
        // SAFETY: preserves this method's trusted immutable kernel contract.
        unsafe { Self::load_inner(bundle, priority, None) }
    }

    /// # Safety
    /// All of `load`'s requirements apply. The supplied admission must come from
    /// independently authenticated original/current issuer evidence, never from
    /// the model's own certificate. Host ISA is checked before native loading.
    pub unsafe fn load_cached(
        bundle: &'a Bundle,
        priority: u8,
        admission: &openpilot_model_runtime::cached_code::Admission,
    ) -> Result<Self, Error> {
        // SAFETY: the explicit caller supplies both contracts above.
        unsafe { Self::load_inner(bundle, priority, Some(admission)) }
    }

    unsafe fn load_inner(
        bundle: &'a Bundle,
        priority: u8,
        admission: Option<&openpilot_model_runtime::cached_code::Admission>,
    ) -> Result<Self, Error> {
        if (bundle.backend == "cpu-cached-llvm") != admission.is_some() {
            return Err(Error::Contract(
                "cached backend requires explicit matching admission",
            ));
        }
        if bundle.descriptor.kind != Kind::Driver {
            return Err(Error::Contract("driver daemon requires a driver bundle"));
        }
        let transform = driver_transform(bundle.descriptor.camera)?
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        let output = bundle
            .descriptor
            .outputs
            .iter()
            .find(|output| output.name == "model")
            .ok_or(Error::Contract("missing driver model output"))?
            .bytes()?;
        if output % 4 != 0 {
            return Err(Error::Contract("driver output must be float32"));
        }
        let model = match bundle.backend.as_str() {
            // SAFETY: the caller guarantees immutable trusted kernels and buffers.
            "cpu-clang" | "cpu-llvm" => Backend::Cpu(unsafe { CpuModel::load(&bundle.directory) }?),
            // SAFETY: the explicit caller authenticates original/current and
            // immutable kernels; load_cached rechecks the actual host before dlopen.
            "cpu-cached-llvm" => Backend::Cpu(unsafe {
                CpuModel::load_cached(
                    &bundle.directory,
                    admission.ok_or(Error::Contract("cached admission absent"))?,
                )
            }?),
            // SAFETY: the caller guarantees the QCOM kernel memory contract.
            "qcom-cl" => Backend::Qcom(unsafe { QcomModel::load(&bundle.directory, priority) }?),
            _ => return Err(Error::Contract("unsupported driver backend")),
        };
        Ok(Self {
            bundle,
            model,
            frame_bytes: vec![0; bundle.descriptor.nv12.bytes],
            output_bytes: vec![0; output],
            output_values: vec![0.0; output / 4],
            transform,
        })
    }

    pub fn validate_frame(&self, metadata: &VisionMetadata) -> Result<(), Error> {
        let descriptor = &self.bundle.descriptor;
        if [metadata.width, metadata.height] != descriptor.camera.map(|v| v as usize)
            || metadata.stride != descriptor.nv12.stride
            || metadata.uv_offset != descriptor.nv12.stride * descriptor.nv12.y_height
            || metadata.len != descriptor.nv12.bytes
        {
            return Err(Error::Contract(
                "camera layout does not match driver artifact",
            ));
        }
        Ok(())
    }

    pub fn infer(&mut self, frame: &VisionFrame<'_>, calibration: [f32; 3]) -> Result<f32, Error> {
        self.validate_frame(frame.metadata())?;
        let calibration: Vec<u8> = calibration.into_iter().flat_map(f32::to_ne_bytes).collect();
        self.model.write("calib", &calibration)?;
        let gpu_start = Instant::now();
        frame.copy_into(&mut self.frame_bytes)?;
        self.model.write("frame", &self.frame_bytes)?;
        self.model.write("transform", &self.transform)?;
        self.model.execute("prepare")?;
        self.model.execute("model")?;
        self.model.read(&mut self.output_bytes)?;
        for (value, bytes) in self
            .output_values
            .iter_mut()
            .zip(self.output_bytes.chunks_exact(4))
        {
            *value = f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        Ok(gpu_start.elapsed().as_secs_f32())
    }

    pub fn message(&self, timing: DriverTiming, raw: bool) -> Result<Vec<u8>, Error> {
        let outputs = RawOutputs::new(
            &self.output_values,
            &self.bundle.descriptor.metadata.output_slices,
        )?;
        let prediction = DriverPrediction::parse(&outputs)?;
        Ok(driver_wire::encode(
            &prediction,
            timing,
            if raw { &self.output_bytes } else { &[] },
        ))
    }
}
