use openpilot_usbgpu::{
    bus_lock::BusLock,
    clock::WallClock,
    stock_asm::StockAsm,
    transport::{BulkResult, Control, Description, Setup, Transfer, Transport},
    usb3::Usb3,
    Error,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufRead},
};
struct Fixture {
    registers: HashMap<u32, u8>,
    trace: Vec<Value>,
}
impl Fixture {
    fn execute(&mut self, cdb: &[u8], read: usize, write: Option<&[u8]>) -> Vec<u8> {
        self.trace
            .push(json!({"cdb":cdb,"read":read,"write":write}));
        let address = if cdb.len() == 6 {
            ((u32::from(cdb[2]) << 16) | (u32::from(cdb[3]) << 8) | u32::from(cdb[4])) & 0x1ffff
        } else {
            0
        };
        if cdb[0] == 0xe5 {
            self.registers.insert(address, cdb[1]);
            if address == 0xb296 && cdb[1] == 4 {
                let format = self.registers[&0xb210];
                let enable = self.registers[&0xb217];
                self.registers.insert(0xb296, 2);
                self.registers.insert(0xb284, u8::from(format & 0x40 == 0));
                self.registers.insert(0xb22a, 0);
                self.registers.insert(
                    0xb22b,
                    if format & 0xbe == 4 {
                        4
                    } else {
                        enable.count_ones() as u8
                    },
                );
                if format & 0x40 == 0 {
                    for (i, value) in [0x12, 0x34, 0x56, 0x78].into_iter().enumerate() {
                        self.registers.insert(0xb220 + i as u32, value);
                    }
                }
            }
        }
        (0..read)
            .map(|offset| {
                *self
                    .registers
                    .get(&(address + offset as u32))
                    .unwrap_or(&((address + offset as u32) as u8))
            })
            .collect()
    }
}
impl Transport for Fixture {
    fn describe(&self) -> Result<Description, Error> {
        Ok(Description {
            bus: 1,
            address: 1,
            product: b"stock".to_vec(),
        })
    }
    fn setup(&mut self, _: Setup, _: i32, _: i32) -> Result<i32, Error> {
        Ok(0)
    }
    fn streams(&mut self, _: &[u8], count: u32) -> Result<i32, Error> {
        Ok(count as i32)
    }
    fn control(&mut self, _: Control, _: &mut [u8]) -> Result<i32, Error> {
        unreachable!()
    }
    fn bulk(&mut self, _: u8, _: &mut [u8], _: u32) -> Result<BulkResult, Error> {
        unreachable!()
    }
    fn batch(&mut self, transfers: &mut [Transfer]) -> Result<(), Error> {
        let mut cursor = 0;
        while cursor < transfers.len() {
            let packet = transfers[cursor].data.clone();
            assert_eq!(transfers[cursor].endpoint, 4);
            let length = if packet[16] == 0x8a { 16 } else { 6 };
            let cdb = &packet[16..16 + length];
            let start = cursor;
            cursor += 2;
            let read = if cursor < transfers.len() && transfers[cursor].endpoint == 0x81 {
                let i = cursor;
                cursor += 1;
                Some(i)
            } else {
                None
            };
            let write = if cursor < transfers.len() && transfers[cursor].endpoint == 2 {
                let i = cursor;
                cursor += 1;
                Some(i)
            } else {
                None
            };
            let result = self.execute(
                cdb,
                read.map_or(0, |i| transfers[i].data.len()),
                write.map(|i| transfers[i].data.as_slice()),
            );
            if let Some(i) = read {
                transfers[i].data.copy_from_slice(&result);
            }
            for transfer in &mut transfers[start..cursor] {
                transfer.status = 0;
                transfer.actual = transfer.data.len() as u32;
            }
        }
        Ok(())
    }
    fn error_text(&self, _: i32) -> String {
        "fixture".into()
    }
}
fn main() {
    for line in io::stdin().lock().lines() {
        let operations: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let usb = Usb3::new(
            Fixture {
                registers: HashMap::new(),
                trace: Vec::new(),
            },
            WallClock::default(),
            BusLock::open(&temp.path().join("lock")).unwrap(),
            false,
        )
        .unwrap();
        let mut controller = StockAsm::new(usb).unwrap();
        let mut values = Vec::new();
        for op in operations.as_array().unwrap() {
            let address = op.get("address").and_then(Value::as_u64).unwrap_or(0);
            let length = op.get("length").and_then(Value::as_u64).unwrap_or(0) as usize;
            let value = match op["kind"].as_str().unwrap() {
                "read" => json!(controller
                    .read(
                        address as u32,
                        length,
                        op.get("stride").and_then(Value::as_u64).unwrap_or(255) as u8
                    )
                    .unwrap()),
                "write" => {
                    controller
                        .write(
                            address as u32,
                            &op["data"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_u64().unwrap() as u8)
                                .collect::<Vec<_>>(),
                            op["ignore_cache"].as_bool().unwrap(),
                        )
                        .unwrap();
                    Value::Null
                }
                "cache" => {
                    controller.cache_range(address, length as u64);
                    Value::Null
                }
                "request" => json!(controller
                    .request(
                        op["format"].as_u64().unwrap() as u8,
                        address,
                        op.get("value").and_then(Value::as_u64).map(|v| v as u32),
                        op["size"].as_u64().unwrap() as u8
                    )
                    .unwrap()),
                "memory_write" => {
                    controller
                        .memory_write(
                            address,
                            &op["data"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_u64().unwrap() as u32)
                                .collect::<Vec<_>>(),
                            op["size"].as_u64().unwrap() as u8,
                        )
                        .unwrap();
                    Value::Null
                }
                "scsi_write" => {
                    controller
                        .scsi_write(
                            &op["data"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_u64().unwrap() as u8)
                                .collect::<Vec<_>>(),
                            op.get("lba").and_then(Value::as_u64).unwrap_or(0),
                        )
                        .unwrap();
                    Value::Null
                }
                _ => panic!("invalid operation"),
            };
            values.push(value);
        }
        println!(
            "{}",
            json!({"values":values,"trace":controller.usb.transport.trace})
        );
    }
}
