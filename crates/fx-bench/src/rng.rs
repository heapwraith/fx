//! Tiny deterministic PRNG (SplitMix64). Fixtures must be identical across runs and
//! machines, so we avoid any OS entropy and any external crate whose output could
//! change between versions.

/// Fixed seed for every fixture (docs/PERFORMANCE.md: "semente fixa").
pub const FIXTURE_SEED: u64 = 0x00F1_F1F1_2026_1121;

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    #[cfg(test)]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Independent stream derived from this seed, e.g. one per fixture or per item,
    /// so parallel generation stays deterministic regardless of thread scheduling.
    pub fn derive(seed: u64, stream: u64) -> Self {
        let mut r = Self(seed ^ stream.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        r.next_u64();
        r
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish value in `0..n` (`n > 0`); modulo bias is irrelevant here.
    pub fn below(&mut self, n: usize) -> usize {
        debug_assert!(n > 0);
        (self.next_u64() % n as u64) as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let a: Vec<u64> = {
            let mut r = Rng::new(42);
            (0..8).map(|_| r.next_u64()).collect()
        };
        let b: Vec<u64> = {
            let mut r = Rng::new(42);
            (0..8).map(|_| r.next_u64()).collect()
        };
        assert_eq!(a, b);
        let mut c = Rng::new(43);
        assert_ne!(a[0], c.next_u64());
    }

    #[test]
    fn derived_streams_differ() {
        assert_ne!(
            Rng::derive(FIXTURE_SEED, 1).next_u64(),
            Rng::derive(FIXTURE_SEED, 2).next_u64()
        );
    }
}
