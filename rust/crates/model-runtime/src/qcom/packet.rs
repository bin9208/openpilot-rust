use super::{invalid, Dispatch, ProgramImage};
use crate::Error;

const CP_EVENT_WRITE: u32 = 0x46;
const CP_LOAD_STATE6_FRAG: u32 = 0x34;
const CP_RUN_OPENCL: u32 = 0x31;
const CP_SET_MARKER: u32 = 0x65;
const CP_WAIT_FOR_IDLE: u32 = 0x26;
const CP_WAIT_MEM_WRITES: u32 = 0x12;
const REG_A6XX_SP_CS_CNTL_0: u32 = 0xa9b0;
const REG_A6XX_SP_CS_CONFIG: u32 = 0xa9bb;
const REG_A6XX_SP_CS_INSTR_SIZE: u32 = 0xa9bc;
const REG_A6XX_SP_CS_NDRANGE_0: u32 = 0xb990;
const REG_A6XX_SP_CS_PVT_MEM_STACK_OFFSET: u32 = 0xa9bd;
const REG_A6XX_SP_CS_SAMPLER_BASE: u32 = 0xa9e2;
const REG_A6XX_SP_CS_TEXMEMOBJ_BASE: u32 = 0xa9e6;
const REG_A6XX_SP_CS_TSIZE: u32 = 0xa9ba;
const REG_A6XX_SP_CS_UAV_BASE: u32 = 0xa9f2;
const REG_A6XX_SP_CS_USIZE: u32 = 0xaa00;
const REG_A6XX_SP_MODE_CNTL: u32 = 0xab00;
const REG_A6XX_SP_PERFCTR_SHADER_MASK: u32 = 0xae0f;
const REG_A6XX_SP_REG_PROG_ID_0: u32 = 0xb983;
const REG_A6XX_SP_UPDATE_CNTL: u32 = 0xbb08;
const REG_A6XX_TPL1_CS_BORDER_COLOR_BASE: u32 = 0xb180;
const REG_A6XX_TPL1_DBG_ECO_CNTL: u32 = 0xb600;
const REG_A6XX_TPL1_MODE_CNTL: u32 = 0xb309;
const CP_TYPE4_PKT: u32 = 0x40000000;
const CP_TYPE7_PKT: u32 = 0x70000000;

#[derive(Default)]
struct Queue(Vec<u32>);

fn parity(value: u32) -> u32 {
    (value.count_ones() & 1) ^ 1
}
fn address(value: u64) -> [u32; 2] {
    [value as u32, (value >> 32) as u32]
}

impl Queue {
    fn command(&mut self, opcode: u32, values: &[u32]) {
        let count = values.len() as u32;
        self.0
            .push(CP_TYPE7_PKT | count | parity(count) << 15 | opcode << 16 | parity(opcode) << 23);
        self.0.extend_from_slice(values);
    }
    fn register(&mut self, register: u32, values: &[u32]) {
        let count = values.len() as u32;
        self.0.push(
            CP_TYPE4_PKT | count | parity(count) << 7 | register << 8 | parity(register) << 27,
        );
        self.0.extend_from_slice(values);
    }
    fn flush(&mut self, dummy: u64) {
        let [low, high] = address(dummy);
        self.command(CP_EVENT_WRITE, &[4, low, high, 0]);
    }
    fn load(&mut self, state: u32, block: u32, units: u32, pointer: u64) {
        let [low, high] = address(pointer);
        self.command(
            CP_LOAD_STATE6_FRAG,
            &[state << 14 | 2 << 16 | block << 18 | units << 22, low, high],
        );
    }
}

impl ProgramImage {
    pub fn dispatch(&self, dispatch: &Dispatch) -> Result<Vec<u32>, Error> {
        let Dispatch {
            program,
            stack,
            border,
            dummy,
            args,
            global,
            local,
        } = *dispatch;
        let threads = local
            .into_iter()
            .try_fold(1_u32, |total, value| total.checked_mul(value))
            .ok_or_else(invalid)?;
        if threads == 0
            || threads > self.max_threads
            || local.iter().any(|&value| value > 1024)
            || global.iter().any(|value| {
                !value.is_finite() || *value <= 0.0 || value.ceil() > f64::from(u32::MAX)
            })
        {
            return Err(invalid());
        }
        let mut size = [0; 3];
        for index in 0..3 {
            let dimension = global[index] * f64::from(local[index]);
            if dimension > f64::from(u32::MAX) || dimension < 1.0 {
                return Err(invalid());
            }
            size[index] = dimension as u32;
        }
        let mut queue = Queue::default();
        queue.command(CP_SET_MARKER, &[8]);
        queue.register(REG_A6XX_SP_UPDATE_CNTL, &[0x60]);
        queue.register(REG_A6XX_SP_UPDATE_CNTL, &[0]);
        queue.register(REG_A6XX_SP_CS_TSIZE, &[0x80]);
        queue.register(REG_A6XX_SP_CS_USIZE, &[0x40]);
        queue.register(REG_A6XX_SP_MODE_CNTL, &[2]);
        queue.register(REG_A6XX_SP_PERFCTR_SHADER_MASK, &[0x20]);
        queue.register(REG_A6XX_TPL1_MODE_CNTL, &[1]);
        queue.register(REG_A6XX_TPL1_DBG_ECO_CNTL, &[0]);
        queue.command(CP_WAIT_FOR_IDLE, &[]);
        queue.register(
            REG_A6XX_SP_CS_NDRANGE_0,
            &[
                3 | (local[0] - 1) << 2 | (local[1] - 1) << 12 | (local[2] - 1) << 22,
                size[0],
                0,
                size[1],
                0,
                size[2],
                0,
                0xccc0cf,
                0xfc,
                global[0].ceil() as u32,
                global[1].ceil() as u32,
                global[2].ceil() as u32,
            ],
        );
        let [program_low, program_high] = address(program);
        let [stack_low, stack_high] = address(stack);
        queue.register(
            REG_A6XX_SP_CS_CNTL_0,
            &[
                self.hregs << 1 | self.fregs << 7 | self.brnchstck << 14,
                2 << 5 | self.shared_size,
                0,
                self.prg_offset,
                program_low,
                program_high,
                self.pvtmem_size_per_item,
                stack_low,
                stack_high,
                self.pvtmem_size_total,
            ],
        );
        queue.load(1, 13, 256, args);
        queue.load(0, 13, self.image_size.div_ceil(128), program);
        queue.register(
            REG_A6XX_SP_REG_PROG_ID_0,
            &[0xfcfcfcfc, 0xfcfcfcfc, 0xfcfcfcfc, 0xfc, 256],
        );
        queue.register(REG_A6XX_SP_CS_PVT_MEM_STACK_OFFSET, &[self.hw_stack_offset]);
        queue.register(REG_A6XX_SP_CS_INSTR_SIZE, &[self.image_size / 4]);
        if self.samp_cnt > 0 {
            let pointer = args
                .checked_add(u64::from(self.samp_off))
                .ok_or_else(invalid)?;
            queue.load(0, 5, self.samp_cnt, pointer);
            queue.register(REG_A6XX_SP_CS_SAMPLER_BASE, &address(pointer));
            queue.register(REG_A6XX_TPL1_CS_BORDER_COLOR_BASE, &address(border));
        }
        if self.tex_cnt > 0 {
            let pointer = args
                .checked_add(u64::from(self.tex_off))
                .ok_or_else(invalid)?;
            queue.load(1, 5, self.tex_cnt.min(16), pointer);
            queue.register(REG_A6XX_SP_CS_TEXMEMOBJ_BASE, &address(pointer));
        }
        if self.ibo_cnt > 0 {
            let pointer = args
                .checked_add(u64::from(self.ibo_off))
                .ok_or_else(invalid)?;
            queue.load(3, 13, self.ibo_cnt, pointer);
            queue.register(REG_A6XX_SP_CS_UAV_BASE, &address(pointer));
        }
        queue.register(
            REG_A6XX_SP_CS_CONFIG,
            &[256 | self.samp_cnt << 17 | self.tex_cnt << 9 | self.ibo_cnt << 22],
        );
        queue.command(CP_RUN_OPENCL, &[0]);
        queue.flush(dummy);
        Ok(queue.0)
    }

    pub fn memory_barrier(dummy: u64) -> Vec<u32> {
        let mut queue = Queue::default();
        queue.flush(dummy);
        queue.command(CP_EVENT_WRITE, &[0x31]);
        queue.command(CP_WAIT_MEM_WRITES, &[]);
        queue.command(CP_WAIT_FOR_IDLE, &[]);
        queue.0
    }
}
