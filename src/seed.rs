use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
pub(crate) struct Rng32 {
    state: u64,
}

impl Rng32 {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x as u32
    }

    pub(crate) fn next_f32(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    pub(crate) fn range_usize(&mut self, min_inclusive: usize, max_inclusive: usize) -> usize {
        if max_inclusive <= min_inclusive {
            return min_inclusive;
        }
        let span = (max_inclusive - min_inclusive + 1) as u32;
        min_inclusive + (self.next_u32() % span) as usize
    }

    pub(crate) fn choose_index(&mut self, len: usize) -> usize {
        self.range_usize(0, len.saturating_sub(1))
    }
}

pub(crate) fn seed_hash(seed: &str) -> u64 {
    let mut h = 0u64;
    for byte in seed.bytes() {
        h = h.wrapping_mul(31).wrapping_add(byte as u64);
    }
    h
}

pub fn validate_seed(seed: &str) -> bool {
    seed.len() == 8
        && seed
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

pub fn generate_seed() -> String {
    let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut rng = Rng32::new(time_seed ^ 0x9E37_79B9_7F4A_7C15);
    let mut out = String::new();
    for _ in 0..8 {
        out.push(chars[rng.choose_index(chars.len())] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_seed_shape() {
        assert!(validate_seed("ABCD1234"));
        assert!(!validate_seed("abc123"));
        assert!(!validate_seed("ABCD-123"));
    }
}
