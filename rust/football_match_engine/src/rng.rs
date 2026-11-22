use crate::fingerprints;

#[derive(Debug, Clone)]
pub struct DeterministicRng { root_seed: u64, state: u64 }

impl DeterministicRng {
    pub fn new(seed: i64) -> Self { Self {root_seed: seed as u64, state: seed as u64} }
    pub fn root_seed(&self) -> i64 { self.root_seed as i64 }
    pub fn state(&self) -> i64 { self.state as i64 }

    pub fn next_long(&mut self) -> i64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        (z ^ (z >> 31)) as i64
    }
    pub fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        (((self.next_long() as u64) >> 1) % (bound as u64)) as i32
    }
    pub fn next_boolean(&mut self) -> bool { (self.next_long() & 1) != 0 }
    pub fn next_double(&mut self) -> f64 {
        (((self.next_long() as u64) >> 11) as f64) * (1.0_f64 / 9007199254740992.0_f64)
    }
    pub fn fork(&self, channel: &str) -> Self { Self::new(derive_seed(self.root_seed as i64, channel)) }
}

pub fn derive_seed(seed: i64, channel: &str) -> i64 {
    let mut hash = fingerprints::start();
    hash = fingerprints::mix_i64(hash, seed);
    hash = fingerprints::mix_string(hash, Some(channel));
    avalanche(hash) as i64
}

fn avalanche(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51afd7ed558ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ceb9fe1a85ec53);
    value ^= value >> 33;
    value
}
