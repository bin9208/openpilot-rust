use crate::usb_model::{Controls, UsbModel};
use crate::Error;
use openpilot_model_runtime::{
    catalog::{Bundle, Kind},
    qcom::QcomModel,
    CpuModel,
};
use openpilot_modeld::{inputs::PolicyInputs, parse::RawOutputs, prediction::DrivingPrediction};
use openpilot_msgq::VisionMetadata;

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

    fn read(&self, name: &str, bytes: &mut [u8]) -> Result<(), Error> {
        match self {
            Self::Cpu(model) => model.read_output(name, bytes)?,
            Self::Qcom(model) => {
                let source = model.read_output(name)?;
                if source.len() != bytes.len() {
                    return Err(Error::Contract("model output size changed"));
                }
                bytes.copy_from_slice(source);
            }
        }
        Ok(())
    }
}

pub struct DrivingRuntime<'a> {
    bundle: &'a Bundle,
    backend: Backend,
    pub inputs: PolicyInputs,
    input_bytes: Vec<u8>,
    output_bytes: Vec<u8>,
    output_values: Vec<f32>,
    hidden: [usize; 2],
    usb: Option<UsbModel>,
}

impl<'a> DrivingRuntime<'a> {
    /// # Safety
    /// The bundle must contain trusted immutable native kernels satisfying the CPU/QCOM load contracts.
    pub unsafe fn load(bundle: &'a Bundle, priority: u8) -> Result<Self, Error> {
        if bundle.descriptor.kind != Kind::Driving {
            return Err(Error::Contract("driving daemon requires a driving bundle"));
        }
        let hidden = *bundle
            .descriptor
            .metadata
            .output_slices
            .get("hidden_state")
            .ok_or(Error::Contract("missing hidden state"))?;
        let count = hidden[1]
            .checked_sub(hidden[0])
            .ok_or(Error::Contract("invalid hidden state"))?;
        let inputs = PolicyInputs::new(count)?;
        let output = bundle
            .descriptor
            .outputs
            .iter()
            .find(|output| output.name == "model")
            .ok_or(Error::Contract("missing model output"))?
            .bytes()?;
        if output % 4 != 0 || hidden[1] > output / 4 {
            return Err(Error::Contract("invalid float32 model output"));
        }
        let backend = match bundle.backend.as_str() {
            // SAFETY: the caller guarantees trusted immutable executable kernels.
            "cpu-clang" | "cpu-llvm" => Backend::Cpu(unsafe { CpuModel::load(&bundle.directory) }?),
            // SAFETY: the caller guarantees the QCOM kernel memory contract.
            "qcom-cl" => Backend::Qcom(unsafe { QcomModel::load(&bundle.directory, priority) }?),
            _ => return Err(Error::Contract("unsupported driving backend")),
        };
        Ok(Self {
            bundle,
            backend,
            inputs,
            input_bytes: vec![0; (count + 12) * 4],
            output_bytes: vec![0; output],
            output_values: vec![0.0; output / 4],
            hidden,
            usb: None,
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
                "camera layout does not match driving artifact",
            ));
        }
        Ok(())
    }

    pub fn infer(
        &mut self,
        main: &[u8],
        extra: &[u8],
        transforms: [[f32; 9]; 2],
        prepare_only: bool,
    ) -> Result<Option<DrivingPrediction>, Error> {
        if let Some(usb) = &mut self.usb {
            return usb.infer(main, extra, transforms).map(Some);
        }
        self.backend.write("frame", main)?;
        self.backend.write("big_frame", extra)?;
        for (name, matrix) in ["tfm", "big_tfm"].into_iter().zip(transforms) {
            let mut bytes = [0; 36];
            for (target, value) in bytes.chunks_exact_mut(4).zip(matrix) {
                target.copy_from_slice(&value.to_ne_bytes());
            }
            self.backend.write(name, &bytes)?;
        }
        for (target, value) in self
            .input_bytes
            .chunks_exact_mut(4)
            .zip(self.inputs.packed())
        {
            target.copy_from_slice(&value.to_ne_bytes());
        }
        self.backend.write("packed_npy_inputs", &self.input_bytes)?;
        self.backend.execute("prepare")?;
        if prepare_only {
            return Ok(None);
        }
        self.backend.execute("policy")?;
        self.backend.read("model", &mut self.output_bytes)?;
        for (value, bytes) in self
            .output_values
            .iter_mut()
            .zip(self.output_bytes.chunks_exact(4))
        {
            *value = f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        self.inputs
            .set_features(&self.output_values[self.hidden[0]..self.hidden[1]])?;
        let outputs = RawOutputs::new(
            &self.output_values,
            &self.bundle.descriptor.metadata.output_slices,
        )?;
        Ok(Some(DrivingPrediction::parse(&outputs)?))
    }

    pub fn warp_for_jetlink(
        &mut self,
        main: &[u8],
        extra: &[u8],
        transforms: [[f32; 9]; 2],
    ) -> Result<Vec<u8>, Error> {
        if self.bundle.backend != "qcom-cl"
            || !transforms.as_flattened().iter().all(|v| v.is_finite())
        {
            return Err(Error::Contract(
                "Jetlink requires the verified QCOM warp geometry",
            ));
        }
        let warped = self
            .bundle
            .descriptor
            .outputs
            .iter()
            .find(|output| output.name == "warped")
            .ok_or(Error::Contract("missing compiled warp output"))?;
        if warped.bytes()? != openpilot_jetlink::contract::WARPED_BYTES {
            return Err(Error::Contract("Jetlink warped shape"));
        }
        self.backend.write("frame", main)?;
        self.backend.write("big_frame", extra)?;
        for (name, matrix) in ["tfm", "big_tfm"].into_iter().zip(transforms) {
            let mut bytes = [0; 36];
            for (target, value) in bytes.chunks_exact_mut(4).zip(matrix) {
                target.copy_from_slice(&value.to_ne_bytes());
            }
            self.backend.write(name, &bytes)?;
        }
        self.backend.execute("prepare")?;
        let mut bytes = vec![0; openpilot_jetlink::contract::WARPED_BYTES];
        self.backend.read("warped", &mut bytes)?;
        Ok(bytes)
    }

    pub fn raw_predictions(&self) -> &[u8] {
        self.usb
            .as_ref()
            .map_or(&self.output_bytes, |usb| usb.client.raw_output())
    }
    pub fn update_inputs(&mut self, controls: Controls) {
        match &mut self.usb {
            Some(usb) => usb.update(controls),
            None => controls.apply(&mut self.inputs),
        }
    }
    pub fn enable_usb(&mut self, model: UsbModel) {
        self.usb = Some(model);
    }
    pub fn uses_usb(&self) -> bool {
        self.usb.is_some()
    }
    pub fn disable_usb(&mut self) {
        if let Some(usb) = self.usb.take() {
            usb.controls.apply(&mut self.inputs);
        }
    }
}
