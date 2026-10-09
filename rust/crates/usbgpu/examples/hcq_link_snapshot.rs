use openpilot_usbgpu::{
    hcq_model::{Allocation, Device, Model, Request},
    hcq_vm::{Function, Host, Memory},
    Error,
};
use std::{collections::BTreeMap, path::Path};

#[derive(Default)]
struct OwnedDevice(BTreeMap<u64, Vec<u8>>);
impl Host for OwnedDevice {
    fn poll(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn call(&mut self, _: Function, _: &[u64], _: &mut Memory) -> Result<u64, Error> {
        Err(Error::Contract("link snapshot must not dispatch"))
    }
}
impl Device for OwnedDevice {
    fn allocate(&mut self, request: Request<'_>) -> Result<Allocation, Error> {
        let key = u64::try_from(self.0.len()).map_err(|_| Error::Contract("fixture key"))?;
        let bytes = usize::try_from(request.bytes).map_err(|_| Error::Contract("fixture size"))?;
        self.0.insert(key, vec![0; bytes]);
        Ok(Allocation {
            key,
            device: 0x1000_0000 + key * (1 << 28),
            host: 0x100_0000_0000 + key * (1 << 28),
            bytes: request.bytes,
        })
    }
    fn write(&mut self, allocation: Allocation, offset: u64, data: &[u8]) -> Result<(), Error> {
        let offset = usize::try_from(offset).map_err(|_| Error::Contract("fixture offset"))?;
        self.0
            .get_mut(&allocation.key)
            .and_then(|bytes| bytes.get_mut(offset..offset + data.len()))
            .ok_or(Error::Contract("fixture write range"))?
            .copy_from_slice(data);
        Ok(())
    }
    fn read(&mut self, allocation: Allocation, data: &mut [u8]) -> Result<(), Error> {
        data.copy_from_slice(
            self.0
                .get(&allocation.key)
                .ok_or(Error::Contract("fixture read key"))?,
        );
        Ok(())
    }
    fn synchronize(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(Error::Contract("expected descriptor, model, snapshot"));
    }
    let mut model = Model::load(
        &std::fs::read(&args[0])?,
        Path::new(&args[1]),
        OwnedDevice::default(),
    )?;
    std::fs::write(
        &args[2],
        serde_json::to_vec_pretty(&model.fixture_link_snapshot()?)?,
    )?;
    Ok(())
}
