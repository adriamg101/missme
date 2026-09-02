//! Predictable random-looking values built from a seed plus whatever
//! coordinates identify a cell, such as a RAM block address or a cache
//! set/way pair. Nothing here is ever saved: a value is worked out fresh
//! every time it's needed, whether that's for showing a set or block on
//! screen, or for a real access touching it. This is what lets "some
//! percentage of cells start out pre-loaded with garbage" work even over an
//! address space far too big to fill in ahead of time.
//!
//! The user never enters a seed themselves, so the real app code never uses
//! a fixed one. Instead it draws a fresh one from `generate_seed()` once,
//! when a RAM store or cache is created, and keeps using that same seed for
//! the whole life of that store or cache. So "always the same" here means
//! stable for one simulation run, not the same across different runs.
//! `hash`, `percent_roll`, `byte`, and `value` all take the seed as an
//! argument rather than baking it in, so tests can fix it and check
//! specific results.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// A fresh, unpredictable seed drawn from the operating system's own
/// randomness (through the same mechanism Rust's hash maps use to randomize
/// themselves). It's not meant to be cryptographically secure, but it's
/// random enough that the garbage pattern won't look the same every run.
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

/// Combines a seed and a list of coordinates into one repeatable 64-bit
/// value. Callers should start `coords` with a small tag number unique to
/// that call site, so different features using this module never end up
/// with the same result for the same coordinates.
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
        // This isn't guaranteed for every possible input, but it holds for
        // these specific ones, and failing it would mean the mixing
        // function stopped actually mixing its inputs.
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
        // Just a sanity check, not rigorous proof: across many coordinates,
        // a dirty-percent style threshold should select roughly that
        // proportion, not something wildly off.
        let hits = (0..10_000u64).filter(|&i| percent_roll(9, &[9, i]) < 30).count();
        assert!((2_500..3_500).contains(&hits), "expected roughly 30% hits, got {hits}");
    }

    #[test]
    fn generate_seed_does_not_reliably_repeat() {
        // Not a hard guarantee, but if the seed were a fixed constant (the
        // bug this test guards against) it would fail every single time.
        let seeds: std::collections::HashSet<u64> = (0..8).map(|_| generate_seed()).collect();
        assert!(seeds.len() > 1, "generate_seed() looks fixed, not random");
    }
}
