use crate::{
    gpu::Gpu,
    local_artifact::Artifact,
    model_graph::Graph,
    runtime_bus::RuntimeBus,
    worker::{Info, Runtime},
    Error,
};

pub struct NativeRuntime<B: RuntimeBus> {
    pub gpu: Gpu<B>,
    pub graph: Graph,
    policy: Vec<u8>,
    offsets: [usize; 4],
    hidden: [usize; 2],
    failed: bool,
    #[cfg(feature = "native-skip-miri")]
    local: Option<crate::qcom_warp::LocalWarp>,
}
impl<B: RuntimeBus> NativeRuntime<B> {
    pub fn new(mut gpu: Gpu<B>, artifact: Artifact) -> Result<Self, Error> {
        if artifact.qcom_warp.is_some() {
            return Err(Error::Contract(
                "local QCOM warp must be attached before execution",
            ));
        }
        Ok(Self {
            graph: Graph::from_bundle(artifact.graph, &mut gpu)?,
            gpu,
            policy: vec![0; artifact.packed_bytes],
            offsets: artifact.scalar_offsets,
            hidden: artifact.hidden,
            failed: false,
            #[cfg(feature = "native-skip-miri")]
            local: None,
        })
    }
    #[cfg(feature = "native-skip-miri")]
    pub fn with_qcom(
        mut gpu: Gpu<B>,
        artifact: Artifact,
        local: crate::qcom_warp::LocalWarp,
    ) -> Result<Self, Error> {
        if artifact.qcom_warp.is_none() {
            return Err(Error::Contract("local package did not declare QCOM warp"));
        }
        Ok(Self {
            graph: Graph::from_bundle(artifact.graph, &mut gpu)?,
            gpu,
            policy: vec![0; artifact.packed_bytes],
            offsets: artifact.scalar_offsets,
            hidden: artifact.hidden,
            failed: false,
            local: Some(local),
        })
    }
    fn prepare_inner(&mut self, packed: &[u8], info: &Info) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Contract("local worker stopped after failure"));
        }
        #[cfg(feature = "native-skip-miri")]
        if let Some(local) = &mut self.local {
            let warped = local.prepare_split(
                [info.input(packed, "img")?, info.input(packed, "big_img")?],
                packed
                    .get(..72)
                    .ok_or(Error::Contract("local transforms missing"))?,
            )?;
            return self.graph.write(&mut self.gpu, "warped", &warped);
        }
        for (target, source) in [
            ("frame", "img"),
            ("big_frame", "big_img"),
            ("tfm", "tfm"),
            ("big_tfm", "big_tfm"),
        ] {
            self.graph
                .write(&mut self.gpu, target, info.input(packed, source)?)?;
        }
        self.graph.run(&mut self.gpu, "prepare")
    }
    fn run_inner(&mut self, packed: &[u8], info: &Info, output: &mut [u8]) -> Result<(), Error> {
        self.prepare_inner(packed, info)?;
        for (index, name) in ["desire", "traffic_convention", "action_t"]
            .into_iter()
            .enumerate()
        {
            let bytes = info.input(packed, name)?;
            let offset = self.offsets[index];
            self.policy
                .get_mut(offset..offset + bytes.len())
                .ok_or(Error::Contract("local scalar range mismatch"))?
                .copy_from_slice(bytes);
        }
        self.graph
            .write(&mut self.gpu, "packed_npy_inputs", &self.policy)?;
        self.graph.run(&mut self.gpu, "policy")?;
        let bytes = self.graph.read(&mut self.gpu, "model")?;
        if bytes.len() != output.len()
            || bytes.chunks_exact(4).any(|value| {
                !f32::from_le_bytes([value[0], value[1], value[2], value[3]]).is_finite()
            })
        {
            return Err(Error::Contract("invalid local model output"));
        }
        output.copy_from_slice(&bytes);
        let hidden = bytes
            .get(self.hidden[0] * 4..self.hidden[1] * 4)
            .ok_or(Error::Contract("local hidden output range mismatch"))?;
        self.policy
            .get_mut(self.offsets[3]..)
            .filter(|target| target.len() == hidden.len())
            .ok_or(Error::Contract("local hidden feedback size mismatch"))?
            .copy_from_slice(hidden);
        Ok(())
    }
}
impl<B: RuntimeBus> Runtime for NativeRuntime<B> {
    fn run(&mut self, packed: &[u8], info: &Info, output: &mut [u8]) -> Result<(), Error> {
        let result = self.run_inner(packed, info, output);
        self.failed = result.is_err();
        result
    }
    fn prepare(&mut self, packed: &[u8], info: &Info, _: &mut [u8]) -> Result<bool, Error> {
        let result = self.prepare_inner(packed, info);
        self.failed = result.is_err();
        result.map(|()| false)
    }
}
