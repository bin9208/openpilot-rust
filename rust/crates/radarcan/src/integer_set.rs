#[derive(Clone)]
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
    pub fn is_empty(&self) -> bool {
        self.used == 0
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.slots.resize(8, None);
        self.used = 0;
    }

    pub fn contains(&self, value: u32) -> bool {
        self.iter().any(|entry| entry == value)
    }

    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.slots.iter().flatten().copied()
    }

    fn place(&mut self, value: u32) -> bool {
        let mask = self.slots.len() - 1;
        let mut perturb = value as usize;
        let mut index = perturb & mask;
        loop {
            let probes = if index + 9 <= mask { 9 } else { 0 };
            for probe in 0..=probes {
                match &mut self.slots[index + probe] {
                    Some(existing) if *existing == value => return false,
                    Some(_) => {}
                    empty @ None => {
                        *empty = Some(value);
                        return true;
                    }
                }
            }
            perturb >>= 5;
            index = index.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb) & mask;
        }
    }

    fn resize(&mut self, minimum_used: usize) {
        let mut size = 8;
        while size <= minimum_used {
            size *= 2;
        }
        let previous = std::mem::replace(&mut self.slots, vec![None; size]);
        for value in previous.into_iter().flatten() {
            self.place(value);
        }
    }

    pub fn insert(&mut self, value: u32) {
        if self.place(value) {
            self.used += 1;
            if self.used * 5 >= (self.slots.len() - 1) * 3 {
                self.resize(self.used * if self.used > 50000 { 2 } else { 4 });
            }
        }
    }

    pub fn from_arrivals(values: impl IntoIterator<Item = u32>) -> Self {
        let mut output = Self::default();
        for value in values {
            output.insert(value);
        }
        output
    }

    pub fn merge(&mut self, other: &Self) {
        if other.used == 0 {
            return;
        }
        if (self.used + other.used) * 5 >= (self.slots.len() - 1) * 3 {
            self.resize((self.used + other.used) * 2);
        }
        if self.used == 0 {
            if self.slots.len() == other.slots.len() {
                self.slots.clone_from(&other.slots);
            } else {
                for value in other.iter() {
                    self.place(value);
                }
            }
            self.used = other.used;
        } else {
            for value in other.iter() {
                self.insert(value);
            }
        }
    }
}
