//! Integer-set display policy from CPython 3.12.14 Objects/setobject.c, PSF license.
//! Source: https://github.com/python/cpython/blob/v3.12.14/Objects/setobject.c
use super::Error;

pub struct IntegerSet {
    slots: Vec<Option<u32>>,
    used: usize,
}
impl Default for IntegerSet {
    fn default() -> Self {
        Self {
            slots: vec![None; 8],
            used: 0,
        }
    }
}

impl IntegerSet {
    fn place(&mut self, value: u32) -> Result<bool, Error> {
        let mask = self.slots.len() - 1;
        let mut perturb = usize::try_from(value).map_err(|_| Error::Numeric)?;
        let mut index = perturb & mask;
        loop {
            let probes = if index + 9 <= mask { 9 } else { 0 };
            for probe in 0..=probes {
                let slot = self.slots.get_mut(index + probe).ok_or(Error::Numeric)?;
                match slot {
                    Some(existing) if *existing == value => return Ok(false),
                    Some(_) => {}
                    None => {
                        *slot = Some(value);
                        return Ok(true);
                    }
                }
            }
            perturb >>= 5;
            index = index.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb) & mask;
        }
    }

    pub fn insert(&mut self, value: u32) -> Result<(), Error> {
        if !self.place(value)? {
            return Ok(());
        }
        self.used += 1;
        if self.used * 5 < (self.slots.len() - 1) * 3 {
            return Ok(());
        }
        let required = self.used * if self.used > 50000 { 2 } else { 4 };
        let mut size = 8;
        while size <= required {
            size *= 2;
        }
        let previous = std::mem::replace(&mut self.slots, vec![None; size]);
        for value in previous.into_iter().flatten() {
            self.place(value)?;
        }
        Ok(())
    }

    pub fn source_repr(&self) -> String {
        if self.used == 0 {
            return "set()".into();
        }
        format!(
            "{{{}}}",
            self.slots
                .iter()
                .flatten()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
