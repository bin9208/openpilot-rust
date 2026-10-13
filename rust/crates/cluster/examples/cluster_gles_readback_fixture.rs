use openpilot_cluster::renderer::nv12::readback::DirectReadback;
use openpilot_startup_ui::{
    draw::Draw,
    geometry::Rect,
    gles::GlesApi,
    renderer::{RenderTarget, Renderer, TargetInfo},
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

const FRAMEBUFFER: u32 = 0x8D40;
const PACK_BUFFER: u32 = 0x88EB;
const ALIGNMENT: u32 = 0x0D05;
const BINDINGS: [u32; 3] = [0x8CA6, 0x88ED, ALIGNMENT];

struct Fixture {
    renderer: Renderer,
    readback: DirectReadback,
    control: GlesApi,
    targets: [RenderTarget; 3],
    saved_buffer: u32,
    output: PathBuf,
    records: Vec<Value>,
    clock: Instant,
}
impl Fixture {
    fn new(output: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&output)?;
        let mut renderer = Renderer::raw_window((32, 24), "Owned GLES readback", 128, 0)?;
        let targets = [
            renderer.render_target((32, 24))?,
            renderer.render_target((16, 12))?,
            renderer.render_target((8, 8))?,
        ];
        let mut control = GlesApi::open("libGLESv2.so")?;
        let saved_buffer = *control
            .generate_buffers(1)?
            .first()
            .ok_or("no control buffer")?;
        control.bind_buffer(PACK_BUFFER, saved_buffer);
        control.allocate_buffer(PACK_BUFFER, 8, 0x88E1)?;
        let readback = DirectReadback::new()?;
        Ok(Self {
            renderer,
            readback,
            control,
            targets,
            saved_buffer,
            output,
            records: Vec::new(),
            clock: Instant::now(),
        })
    }
    fn now(&self) -> f64 {
        100.0 + self.clock.elapsed().as_secs_f64()
    }
    fn paint(
        &mut self,
        target: usize,
        frame: u16,
    ) -> Result<TargetInfo, Box<dyn std::error::Error>> {
        let target = &self.targets[target];
        self.renderer.begin_target(target)?;
        self.renderer.clear(0)?;
        for y in 0..u16::try_from(target.height)? {
            for x in 0..u16::try_from(target.width)? {
                let color = [
                    u8::try_from((x * 7 + frame * 13) % 256)?,
                    u8::try_from(y * 11 % 256)?,
                    u8::try_from((x + y) * 4 % 256)?,
                    255,
                ];
                self.renderer.rectangle(
                    Rect {
                        x: f32::from(x),
                        y: f32::from(y),
                        width: 1.0,
                        height: 1.0,
                    },
                    u32::from_le_bytes(color),
                );
            }
        }
        self.renderer.end_target();
        self.renderer.target_info(target).map_err(Into::into)
    }
    fn restore_fixture_bindings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let saved = self.renderer.target_info(&self.targets[2])?;
        self.control
            .bind_framebuffer(FRAMEBUFFER, saved.framebuffer);
        self.control.bind_buffer(PACK_BUFFER, self.saved_buffer);
        self.control.pixel_store(ALIGNMENT, 8);
        Ok(())
    }
    fn bindings(&self) -> [i32; 3] {
        BINDINGS.map(|parameter| self.control.integer(parameter))
    }
    fn direct(&mut self, info: TargetInfo, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.restore_fixture_bindings()?;
        let before = self.bindings();
        let mut bytes = vec![0xA5; usize::try_from(info.width * info.height * 4)? + 7];
        self.readback.read_rgba(info, &mut bytes)?;
        let restored = before == self.bindings();
        let path = self.output.join(format!("{name}.rgba"));
        std::fs::write(&path, bytes)?;
        self.records
            .push(json!({"name": name, "path":format!("{name}.rgba"), "restored":restored}));
        Ok(())
    }
    fn enqueue(&mut self, info: TargetInfo, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.restore_fixture_bindings()?;
        let before = self.bindings();
        let accepted = self.readback.enqueue_rgba(info, self.now())?;
        self.records.push(json!({"name":name,"accepted":accepted,"can_enqueue":self.readback.async_can_enqueue(),"restored":before==self.bindings()}));
        Ok(())
    }
    fn copy(&mut self, size: usize, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + std::time::Duration::from_millis(900);
        while !self.readback.async_ready(self.now())? {
            if Instant::now() >= deadline {
                return Err("GPU readback timed out".into());
            }
            std::thread::yield_now();
        }
        self.restore_fixture_bindings()?;
        let before = self.bindings();
        let mut bytes = vec![0xA5; size + 7];
        let copied = self.readback.copy_ready(&mut bytes, self.now())?;
        let path = self.output.join(format!("{name}.rgba"));
        std::fs::write(&path, bytes)?;
        self.records.push(json!({"name":name,"copied":copied,"path":format!("{name}.rgba"),"can_enqueue":self.readback.async_can_enqueue(),"restored":before==self.bindings()}));
        Ok(())
    }
    fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.records.push(json!({"name":"initial","supported":self.readback.async_supported(),"can_enqueue":self.readback.async_can_enqueue(),"ready":self.readback.async_ready(self.now())?}));
        let info = self.paint(0, 0)?;
        self.direct(info, "direct-0")?;
        for frame in 1..=4 {
            let info = self.paint(0, frame)?;
            self.enqueue(info, &format!("enqueue-{frame}"))?;
        }
        for frame in 1..=3 {
            self.copy(32 * 24 * 4, &format!("copy-{frame}"))?;
        }
        let info = self.paint(0, 5)?;
        self.enqueue(info, "enqueue-discarded")?;
        let info = self.paint(1, 6)?;
        self.enqueue(info, "enqueue-resized")?;
        self.copy(16 * 12 * 4, "copy-resized")?;
        self.readback.disable_async()?;
        self.records.push(json!({"name":"disabled","supported":self.readback.async_supported(),"can_enqueue":self.readback.async_can_enqueue(),"ready":self.readback.async_ready(self.now())?}));
        let info = self.paint(1, 7)?;
        self.direct(info, "direct-disabled")?;
        self.readback.close()?;
        self.readback.close()?;
        self.records
            .push(json!({"name":"closed","ready":self.readback.async_ready(self.now())?}));
        self.control.bind_buffer(PACK_BUFFER, 0);
        self.control.bind_framebuffer(FRAMEBUFFER, 0);
        self.control.delete_buffers(&[self.saved_buffer])?;
        std::fs::write(
            self.output.join("result.json"),
            serde_json::to_vec_pretty(&self.records)?,
        )?;
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("cluster_gles_readback_fixture OUTPUT")?;
    Fixture::new(Path::new(&output).to_path_buf())?.run()
}
