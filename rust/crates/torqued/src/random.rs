//! NumPy RandomState MT19937 and random_interval Fisher-Yates permutation.
//! Algorithm provenance: numpy/random/src/mt19937 and distributions.c (BSD-3-Clause).
use crate::Error;

pub struct RandomState {
    state: [u32; 624],
    index: usize,
}
impl RandomState {
    pub fn seeded(seed: u32) -> Self {
        let mut state = [0_u32; 624];
        state[0] = seed;
        for i in 1..624 {
            // i is bounded by the MT19937 state size.
            state[i] = 1812433253_u32
                .wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: 624 }
    }
    pub fn entropy() -> Result<Self, Error> {
        let mut bytes = [0_u8; 624 * 4];
        getrandom::fill(&mut bytes).map_err(|_| Error::Contract("OS entropy unavailable"))?;
        let mut result = Self::seeded(19650218);
        let mut i = 1;
        for (j, chunk) in bytes.chunks_exact(4).enumerate() {
            let key = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            result.state[i] = (result.state[i]
                ^ (result.state[i - 1] ^ (result.state[i - 1] >> 30)).wrapping_mul(1664525))
            .wrapping_add(key)
            .wrapping_add(j as u32);
            i += 1;
            if i == 624 {
                result.state[0] = result.state[623];
                i = 1;
            }
        }
        for _ in 0..623 {
            result.state[i] = (result.state[i]
                ^ (result.state[i - 1] ^ (result.state[i - 1] >> 30)).wrapping_mul(1566083941))
            .wrapping_sub(i as u32);
            i += 1;
            if i == 624 {
                result.state[0] = result.state[623];
                i = 1;
            }
        }
        result.state[0] = 0x80000000;
        Ok(result)
    }
    fn next(&mut self) -> u32 {
        if self.index == 624 {
            for i in 0..624 {
                let y = (self.state[i] & 0x80000000) | (self.state[(i + 1) % 624] & 0x7fffffff);
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ if y & 1 != 0 { 0x9908b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^ (y >> 18)
    }
    pub fn sample_indices(&mut self, population: usize, count: usize) -> Result<Vec<usize>, Error> {
        let max = u32::try_from(population)
            .map_err(|_| Error::Contract("sampling population too large"))?;
        let mut indices: Vec<usize> = (0..population).collect();
        for i in (1..max).rev() {
            let mask = u32::MAX >> i.leading_zeros();
            let j = loop {
                let draw = self.next() & mask;
                if draw <= i {
                    break draw;
                }
            };
            indices.swap(i as usize, j as usize);
        }
        indices.truncate(count.min(population));
        Ok(indices)
    }
}
