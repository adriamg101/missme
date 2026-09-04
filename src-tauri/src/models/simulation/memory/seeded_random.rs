//! Predictable random looking values built from a seed and coordinates like a RAM address or a cache position.
//! Nothing is ever saved, a value is worked out fresh every time so garbage can cover any address space.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// A fresh, unpredictable seed drawn from the operating system.
/// Not meant to be secure, just random enough that the garbage pattern differs each run.
pub fn generate_seed() -> u64 {
    RandomState::new().build_hasher().finish()
}

/// A cheap way to spread out a number's bits so the result looks well
/// mixed. Not cryptographically secure, just good enough for our purposes.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x
}

/// Combines a seed and a list of coordinates into one repeatable value.
/// Callers should start with a small tag unique to that call site, so different features never collide.
fn hash(seed: u64, coords: &[u64]) -> u64 {
    let mut h = seed;
    for &c in coords {
        h = mix(h ^ mix(c));
    }
    h
}

/// A repeatable roll between 0 and 99, used to decide whether an untouched
/// cell counts as dirty for a given dirty percent setting.
pub fn percent_roll(seed: u64, coords: &[u64]) -> u8 {
    (hash(seed, coords) % 100) as u8
}

/// A repeatable random-looking byte, used to fill in garbage content for a
/// pre-loaded dirty RAM cell.
pub fn byte(seed: u64, coords: &[u64]) -> u8 {
    (hash(seed, coords) & 0xff) as u8
}

/// A repeatable random-looking 64-bit value, used for example as a fake tag
/// for a pre-loaded dirty cache line.
pub fn value(seed: u64, coords: &[u64]) -> u64 {
    hash(seed, coords)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_and_coordinates_always_roll_the_same_value() {
        assert_eq!(percent_roll(99, &[1, 42]), percent_roll(99, &[1, 42]));
        assert_eq!(byte(99, &[2, 42]), byte(99, &[2, 42]));
        assert_eq!(value(99, &[3, 42]), value(99, &[3, 42]));
    }

    #[test]
    fn a_different_seed_rolls_a_different_value() {
        assert_ne!(value(1, &[1, 1]), value(2, &[1, 1]));
    }

    #[test]
    fn different_coordinates_usually_roll_different_values() {
        // Not guaranteed for every input, but true for these ones, and a failure here would mean the mixing broke.
        assert_ne!(value(9, &[1, 1]), value(9, &[1, 2]));
        assert_ne!(value(9, &[1, 1]), value(9, &[2, 1]));
    }

    #[test]
    fn percent_roll_stays_within_bounds() {
        for i in 0..1000u64 {
            assert!(percent_roll(9, &[9, i]) < 100);
        }
    }

    #[test]
    fn percent_roll_distribution_roughly_matches_the_requested_threshold() {
        // Just a sanity check across many coordinates, the result should be roughly the requested proportion.
        let hits = (0..10_000u64).filter(|&i| percent_roll(9, &[9, i]) < 30).count();
        assert!((2_500..3_500).contains(&hits), "expected roughly 30% hits, got {hits}");
    }

    #[test]
    fn generate_seed_does_not_reliably_repeat() {
        // Not a hard guarantee, but a fixed constant seed would fail this every single time.
        let seeds: std::collections::HashSet<u64> = (0..8).map(|_| generate_seed()).collect();
        assert!(seeds.len() > 1, "generate_seed() looks fixed, not random");
    }
}
