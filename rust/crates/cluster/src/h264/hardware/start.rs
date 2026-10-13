use super::{Hardware, Packets};
use crate::h264::{native_sender::Sender, sps::PatchCache, vidc, Config, Output};
use std::{fs::File, io};

impl Hardware {
    /// # Errors
    /// Preserves native encoder, output stream, and dump initialization failures.
    pub fn start<S: Output + 'static>(
        encoder: vidc::Config,
        config: Config,
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
    ) -> io::Result<Self> {
        let mut encoder = vidc::Encoder::new(encoder).map_err(io::Error::other)?;
        encoder.open().map_err(io::Error::other)?;
        let sender = Sender::start(
            config.requested_chunk_size,
            config.wait_for_ack,
            config.soft_ack,
            factory,
        )?;
        Self::from_output(encoder, config, sender)
    }
    /// # Errors
    /// Preserves display metadata, configured native encoder, stream and dump startup failures.
    pub fn start_with<S: Output + 'static>(
        mut encoder: vidc::Config,
        mut config: Config,
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
        configure: impl FnOnce(&mut vidc::Config, &mut Config, Option<[u32; 2]>) -> io::Result<()>,
    ) -> io::Result<Self> {
        let mut opened = None;
        let sender = Sender::start_with(
            config.requested_chunk_size,
            config.wait_for_ack,
            config.soft_ack,
            factory,
            |dimensions| {
                configure(&mut encoder, &mut config, dimensions)?;
                let mut encoder = vidc::Encoder::new(encoder).map_err(io::Error::other)?;
                encoder.open().map_err(io::Error::other)?;
                opened = Some(encoder);
                Ok(())
            },
        )?;
        Self::from_output(
            opened.ok_or_else(|| io::Error::other("native H264 encoder startup missing"))?,
            config,
            sender,
        )
    }
    fn from_output(encoder: vidc::Encoder, config: Config, sender: Sender) -> io::Result<Self> {
        let dump = config
            .dump
            .as_ref()
            .map(|path| {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                File::create(path)
            })
            .transpose()?;
        Ok(Self {
            encoder,
            packets: Packets {
                sender,
                cache: PatchCache::default(),
                config,
                dump,
                writes: 0,
                failure: None,
                monitor: None,
            },
            frame: 0,
            closed: false,
        })
    }
}
