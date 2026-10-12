use openpilot_usbgpu::{
    amd_metadata::Catalog,
    kernel::{Kernel, ScratchPlan},
    packets::{ComputePackets, Dispatch},
    Error,
};
use serde_json::{json, Value};
use std::io::{self, BufRead};
fn number(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap()
}
fn run(catalog: &Catalog, v: &Value) -> Result<Value, Error> {
    let gfx = number(v, "gfx") as u8;
    let xccs = number(v, "xccs") as u8;
    let scratch = ScratchPlan::new(catalog, gfx, number(v, "private") as u32, 16, 32, 2, xccs)?;
    let mut aql_descriptor = vec![0; catalog.layout("struct_amd_queue_s")?.size];
    scratch.aql_descriptor(catalog, gfx, 0x210001000000, &mut aql_descriptor)?;
    let kernel = Kernel::load(
        catalog,
        &std::fs::read(v["path"].as_str().unwrap())?,
        gfx,
        64,
    )?;
    let global = [11, 7, 3];
    let local = [8, 4, 2];
    let base = 0x210010000000;
    let arguments = number(v, "arguments");
    let gc = match gfx {
        9 => [9, 4, 3],
        11 => [11, 0, 0],
        _ => [12, 0, 0],
    };
    let nb = match gfx {
        9 => [7, 4, 0],
        11 => [4, 3, 0],
        _ => [6, 3, 1],
    };
    let regs = catalog.queue_registers(gc, nb)?;
    let mut queue = ComputePackets::new(catalog, &regs, gfx, xccs);
    queue.dispatch(Dispatch {
        descriptor: &kernel.descriptor,
        program_base: base,
        arguments,
        global,
        local,
        scratch_address: 0x210001000000,
        scratch_bytes: scratch.total_bytes,
        tmpring: scratch.tmpring,
        waves_per_shader: 0,
    })?;
    let words = queue.words.clone();
    queue.words.clear();
    queue.memory_barrier(nb)?;
    Ok(
        json!({"descriptor":kernel.descriptor,"scratch":scratch,"aql_descriptor":aql_descriptor,"words":words,"barrier":queue.words,
        "dispatch":kernel.descriptor.dispatch_packet(catalog,base,arguments,global,local,false)?,
        "aql":kernel.descriptor.dispatch_packet(catalog,base,arguments,global,local,true)?}),
    )
}
fn main() {
    let catalog = Catalog::bundled().unwrap();
    for line in io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        match run(&catalog, &v) {
            Ok(v) => println!("{v}"),
            Err(e) => println!("{}", json!({"error":e.to_string()})),
        }
    }
}
