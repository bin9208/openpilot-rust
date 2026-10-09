struct Rpc {
    input: io::BufReader<io::Stdin>,
    clock: Cell<u64>,
    custom: bool,
}
impl Rpc {
    fn call(&mut self, value: Value) -> Result<Value, Error> {
        println!("{value}");
        io::stdout().flush()?;
        let mut line = String::new();
        self.input.read_line(&mut line)?;
        let value: Value = serde_json::from_str(&line)?;
        self.clock.set(self.clock.get() + 1);
        if let Some(error) = value.get("error") {
            return Err(Error::Protocol(error.to_string()));
        }
        Ok(value["value"].clone())
    }
    fn number(&mut self, value: Value) -> Result<u64, Error> {
        self.call(value)?
            .as_u64()
            .ok_or(Error::Contract("RPC expected integer"))
    }
    fn bytes(&mut self, value: Value) -> Result<Vec<u8>, Error> {
        self.call(value)?
            .as_array()
            .ok_or(Error::Contract("RPC expected bytes"))?
            .iter()
            .map(|v| {
                u8::try_from(v.as_u64().ok_or(Error::Contract("RPC byte not integer"))?)
                    .map_err(|_| Error::Contract("RPC byte overflow"))
            })
            .collect()
    }
}
impl Bus for Rpc {
    fn mmio_words(&self) -> u64 {
        0x10000000
    }
    fn vram_bar_size(&self) -> u64 {
        std::env::var("USBGPU_FIXTURE_VRAM_BYTES")
            .map(|value| value.parse().expect("fixture VRAM size"))
            .unwrap_or(512 << 20)
    }
    fn read_register(&mut self, index: u64) -> Result<u32, Error> {
        Ok(self.number(json!({"op":"reg_read","index":index}))? as u32)
    }
    fn write_register(&mut self, index: u64, value: u32) -> Result<(), Error> {
        self.call(json!({"op":"reg_write","index":index,"value":value}))?;
        Ok(())
    }
    fn read_vram(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        self.bytes(json!({"op":"vram_read","address":address,"size":size}))
    }
    fn write_vram(&mut self, address: u64, data: &[u8]) -> Result<(), Error> {
        self.call(json!({"op":"vram_write","address":address,"data":data}))?;
        Ok(())
    }
    fn read_vram_scalar(&mut self, address: u64, size: u8) -> Result<u64, Error> {
        self.number(json!({"op":"value_read","address":address,"size":size}))
    }
    fn write_vram_scalar(&mut self, address: u64, size: u8, value: u64) -> Result<(), Error> {
        self.call(json!({"op":"value_write","address":address,"size":size,"value":value}))?;
        Ok(())
    }
    fn write_doorbell(&mut self, index: u64, value: u64) -> Result<(), Error> {
        self.call(json!({"op":"doorbell_write","index":index,"value":value}))?;
        Ok(())
    }
    fn read_config(&mut self, offset: u16, size: u8) -> Result<u32, Error> {
        Ok(self.number(json!({"op":"config_read","offset":offset,"size":size}))? as u32)
    }
    fn write_config(&mut self, offset: u16, size: u8, value: u32) -> Result<(), Error> {
        self.call(json!({"op":"config_write","offset":offset,"size":size,"value":value}))?;
        Ok(())
    }
    fn alloc_sram(&mut self, size: usize) -> Result<(u64, u64), Error> {
        let values = self.call(json!({"op":"alloc_sram","size":size}))?;
        Ok((values[0].as_u64().unwrap(), values[1].as_u64().unwrap()))
    }
    fn write_sram(&mut self, address: u64, data: &[u8]) -> Result<(), Error> {
        self.call(json!({"op":"sram_write","address":address,"data":data}))?;
        Ok(())
    }
    fn read_sram(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        self.bytes(json!({"op":"sram_read","address":address,"size":size}))
    }
    fn now(&self) -> Duration {
        Duration::from_millis(self.clock.get())
    }
    fn sleep(&mut self, duration: Duration) {
        self.call(json!({"op":"sleep","milliseconds":duration.as_millis()}))
            .unwrap();
        self.clock
            .set(self.clock.get() + duration.as_millis() as u64);
    }
}
struct Source(PathBuf);
impl FirmwareSource for Source {
    fn load(&mut self, name: &str, _: &str) -> Result<Vec<u8>, Error> {
        Ok(std::fs::read(self.0.join(name))?)
    }
}
impl openpilot_usbgpu::runtime_bus::RuntimeBus for Rpc {
    fn custom_bridge(&self) -> bool { self.custom }
    fn cache_doorbells(&mut self) -> Result<(), Error> { self.call(json!({"op":"cache_doorbells"}))?; Ok(()) }
    fn cache_vram(&mut self, offset:u64, size:u64) -> Result<(), Error> { self.call(json!({"op":"cache_vram","offset":offset,"size":size}))?; Ok(()) }
    fn read_controller(&mut self,address:u64,size:usize) -> Result<Vec<u8>,Error> { self.bytes(json!({"op":"controller_read","address":address,"size":size})) }
    fn write_controller(&mut self,address:u64,data:&[u8]) -> Result<(),Error> { self.call(json!({"op":"controller_write","address":address,"data":data}))?;Ok(()) }
    fn arm_staging_read(&mut self,size:usize) -> Result<(),Error> { self.call(json!({"op":"arm_read","size":size}))?;Ok(()) }
    fn memory_barrier(&mut self) -> Result<(),Error> { self.call(json!({"op":"barrier"}))?;Ok(()) }
}
