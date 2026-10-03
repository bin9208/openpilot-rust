use super::{u32_at, CommandBuffer, IO_SIZE};

#[derive(Clone, Copy, Debug, Default)]
pub struct Plane {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub slice_height: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IoConfig {
    pub handles: [i32; 3],
    pub offsets: [u32; 3],
    pub planes: [Plane; 3],
    pub format: u32,
    pub color_space: u32,
    pub color_pattern: u32,
    pub bpp: u32,
    pub rotation: u32,
    pub resource: u32,
    pub fence: i32,
    pub early_fence: i32,
    pub auxiliary: CommandBuffer,
    pub direction: u32,
    pub batch_size: u32,
    pub subsample_pattern: u32,
    pub subsample_period: u32,
    pub framedrop_pattern: u32,
    pub framedrop_period: u32,
    pub flag: u32,
}

impl IoConfig {
    pub fn bytes(self) -> [u8; IO_SIZE] {
        let mut bytes = [0; IO_SIZE];
        for index in 0..3 {
            u32_at(&mut bytes, index * 4, self.handles[index] as u32);
            u32_at(&mut bytes, 12 + index * 4, self.offsets[index]);
            let plane = self.planes[index];
            for (field, value) in [plane.width, plane.height, plane.stride, plane.slice_height]
                .into_iter()
                .enumerate()
            {
                u32_at(&mut bytes, 24 + index * 48 + field * 4, value);
            }
        }
        let fields = [
            self.format,
            self.color_space,
            self.color_pattern,
            self.bpp,
            self.rotation,
            self.resource,
            self.fence as u32,
            self.early_fence as u32,
        ];
        for (index, value) in fields.into_iter().enumerate() {
            u32_at(&mut bytes, 168 + index * 4, value);
        }
        bytes[200..224].copy_from_slice(&self.auxiliary.bytes());
        let fields = [
            self.direction,
            self.batch_size,
            self.subsample_pattern,
            self.subsample_period,
            self.framedrop_pattern,
            self.framedrop_period,
            self.flag,
        ];
        for (index, value) in fields.into_iter().enumerate() {
            u32_at(&mut bytes, 224 + index * 4, value);
        }
        bytes
    }
}
