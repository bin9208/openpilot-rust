use openpilot_camera_kernel::{
    AllocationOptions, CallResult, Device, DeviceHandle, DeviceOperation, Error, Fence, Link,
    MemoryPool, MmuHandles, Session,
};
use serde_json::{json, Value};
use std::os::fd::AsFd;

fn result(call: CallResult) -> Value {
    json!([call.code, call.errno])
}

fn run(mode: &str, seed: u32) -> Result<Value, Box<dyn std::error::Error>> {
    let device = Device::open("/dev/camera-fixture-request")?;
    let session = Session(seed as i32);
    let handle = DeviceHandle(seed.wrapping_add(17) as i32);
    let link = Link(seed.wrapping_add(31) as i32);
    let request = (u64::from(seed) << 32) | u64::from(seed ^ 0xa55a);
    let value = match mode {
        "session" => {
            let (call, created) = device.create_session();
            json!([result(call), created.0])
        }
        "destroy_session" => result(device.destroy_session(session)),
        "acquire_sensor" | "acquire_phy" => {
            let acquired = if mode == "acquire_sensor" {
                device.acquire_sensor(session)
            } else {
                device.acquire_phy(session)
            };
            match acquired {
                Ok(value) => json!([true, value.0]),
                Err(Error::Control { .. }) => json!([false, 0]),
                Err(error) => return Err(error.into()),
            }
        }
        "configure" => result(device.configure(session, handle, seed ^ 0xabcd)),
        "start" | "stop" | "release_device" => {
            let operation = match mode {
                "start" => DeviceOperation::Start,
                "stop" => DeviceOperation::Stop,
                _ => DeviceOperation::Release,
            };
            result(device.control_device(operation, session, handle))
        }
        "flush_device" => result(device.flush_device(session, handle)),
        "link" => match device.link(session, handle, DeviceHandle(seed.wrapping_add(19) as i32)) {
            Ok(value) => json!([true, value.0]),
            Err(Error::Control { .. }) => json!([false, 0]),
            Err(error) => return Err(error.into()),
        },
        "activate" | "deactivate" => {
            result(device.activate_link(session, link, mode == "activate"))
        }
        "unlink" => result(device.unlink(session, link)),
        "schedule" => result(device.schedule(session, link, request)),
        "flush_requests" => result(device.flush_requests(session, link)),
        "probe" => result(device.probe_sensor(seed)),
        "release_buffer" => result(device.release_buffer(seed)),
        "fences" => {
            let fences = device.create_fences(seed & 1 != 0);
            json!([
                result(fences.ife.0),
                fences.ife.1 .0,
                fences
                    .bps
                    .map(|(call, fence)| json!([result(call), fence.0]))
            ])
        }
        "wait" => result(device.wait_fence(Fence(seed as i32), request)),
        "destroy_fence" => result(device.destroy_fence(Fence(seed as i32))),
        "poll" => {
            let poll = device.poll_priority(1000);
            json!([result(poll.result), poll.revents])
        }
        "event" => {
            let (call, event) = device.dequeue_event();
            json!([
                result(call),
                event.kind,
                event.id,
                event.session.0,
                event.link,
                event.frame.request_id,
                event.frame.frame_id,
                event.frame.timestamp,
                event.frame.sof_status
            ])
        }
        "imports" => {
            let raw = std::fs::File::open("/dev/camera-fixture-raw")?;
            let yuv = std::fs::File::open("/dev/camera-fixture-yuv")?;
            let imported = device.import_images(
                MmuHandles {
                    device: seed as i32,
                    cdm: 0,
                    icp: seed.wrapping_add(1) as i32,
                },
                seed & 1 != 0,
                (seed & 2 != 0).then(|| raw.as_fd()),
                (seed & 4 != 0).then(|| yuv.as_fd()),
            );
            match imported {
                Ok(value) => json!([true, value.raw, value.yuv, value.yuv_result.map(result)]),
                Err(Error::Control { .. }) => json!([false]),
                Err(error) => return Err(error.into()),
            }
        }
        "allocation" => {
            let mut allocation = device.allocate(AllocationOptions {
                length: 257,
                alignment: 32,
                flags: 0x859,
                mmu: [
                    if seed & 1 != 0 { seed as i32 } else { 0 },
                    if seed & 2 != 0 { -17 } else { 0 },
                ],
            })?;
            allocation.write(1, &seed.to_le_bytes())?;
            allocation.write(253, &seed.wrapping_add(1).to_le_bytes())?;
            let handle = allocation.handle();
            allocation.close()?;
            json!([true, handle])
        }
        "pool" => {
            let mut pool = MemoryPool::new(&device);
            let mut a = pool.lease(64)?;
            let mut b = pool.lease(64)?;
            let mut c = pool.lease(80)?;
            let handles = [a.handle(), b.handle(), c.handle()];
            a.write(0, &seed.to_le_bytes())?;
            b.write(60, &seed.wrapping_add(1).to_le_bytes())?;
            c.write(76, &seed.wrapping_add(2).to_le_bytes())?;
            drop(a);
            drop(b);
            drop(c);
            let mut d = pool.lease(64)?;
            let mut e = pool.lease(64)?;
            let f = pool.lease(80)?;
            let reused = [d.handle(), e.handle(), f.handle()];
            d.write(8, &seed.wrapping_add(3).to_le_bytes())?;
            e.write(12, &seed.wrapping_add(4).to_le_bytes())?;
            drop(e);
            drop(d);
            drop(f);
            pool.close()?;
            json!([handles, reused])
        }
        _ => return Err(format!("unknown mode {mode}").into()),
    };
    Ok(value)
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let output = args.get(1).ok_or("missing mode").and_then(|mode| {
        let seed = args
            .get(2)
            .ok_or("missing seed")?
            .parse()
            .map_err(|_| "invalid seed")?;
        run(mode, seed).map_err(|error| {
            eprintln!("{error}");
            "operation failed"
        })
    });
    match output {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
