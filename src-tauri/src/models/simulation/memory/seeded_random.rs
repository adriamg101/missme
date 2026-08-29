//! Deterministic pseudo-random values derived from an explicit `seed` plus
//! whatever coordinates identify a cell (a RAM block address, a cache
//! set/way pair). Nothing here is ever stored: a value is recomputed fresh
//! every time it's needed (a set/block being displayed, or touched by a
//! real access), which is what lets "some percentage of cells start
//! pre-loaded with garbage" work even over an address space far too large
//! to ever materialize eagerly.
//!
//! No seed is ever entered by the user, so production code never hardcodes
//! one — it draws a fresh one from `generate_seed()` once, when a RAM store
//! or cache is built, and keeps it fixed for that instance's whole
//! lifetime. That's what "always the same" actually means here: stable for
//! the life of one simulation run, not identical across different runs.
//! `hash`/`percent_roll`/`byte`/`value` take the seed as an explicit
//! parameter (rather than a global constant) precisely so tests can pin it
//! down and assert on specific outcomes.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// A fresh, unpredictable seed drawn from the OS's own entropy source
/// (via `std`'s hashmap-randomization mechanism) — not cryptographic, but
/// exactly as random as `HashMap`'s own DoS-resistant hashing, which is
/// plenty for "don't show the same garbage pattern every run".
pub fn generate_seed() -> u64 {
    RandomState::new().build_hasher().finish()
}

/// splitmix64's finalizer — cheap and well-distributed, not cryptographic.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x
}

/// Folds a seed and a list of coordinates into one deterministic 64-bit
/// value. Callers should lead `coords` with a domain tag (a small constant
/// unique to that call site) so unrelated uses of this module never roll
/// the same sequence for the same underlying coordinates.
fn hash(seed: u64, coords: &[u64]) -> u64 {
    let mut h = seed;
    for &c in coords {
        h = mix(h ^ mix(c));
    }
    h
}

/// A deterministic roll in `[0, 100)`, used to decide whether a not-yet-
/// touched cell counts as dirty for a given `dirty_percent` threshold.
pub fn percent_roll(seed: u64, coords: &[u64]) -> u8 {
    (hash(seed, coords) % 100) as u8
}

/// A deterministic pseudo-random byte, used to fill in garbage content for
/// a dirty-seeded RAM cell.
pub fn byte(seed: u64, coords: &[u64]) -> u8 {
    (hash(seed, coords) & 0xff) as u8
}

/// A deterministic pseudo-random 64-bit value, used e.g. as a fake tag for
/// a dirty-seeded phantom cache line.
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
        // Not a hard guarantee for a hash function, but true for these
        // specific inputs — regressing this would indicate `hash` stopped
        // actually mixing its inputs.
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
        // Sanity check, not a statistical proof: across many coordinates, a
        // dirty_percent-style threshold should select roughly that
        // proportion, not something wildly skewed.
        let hits = (0..10_000u64).filter(|&i| percent_roll(9, &[9, i]) < 30).count();
        assert!((2_500..3_500).contains(&hits), "expected roughly 30% hits, got {hits}");
    }

    #[test]
    fn generate_seed_does_not_reliably_repeat() {
        // Not a hard guarantee, but a fixed constant (the bug being fixed
        // here) would fail this immediately, every time.
        let seeds: std::collections::HashSet<u64> = (0..8).map(|_| generate_seed()).collect();
        assert!(seeds.len() > 1, "generate_seed() looks fixed, not random");
    }
}
