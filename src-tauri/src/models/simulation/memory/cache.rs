use std::collections::HashMap;

use super::seeded_random;

#[derive(Debug, Clone)]
pub struct CacheLine {
    pub tag: u64,
    pub valid: bool,
    pub dirty: bool,
    last_used: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupOutcome {
    Hit,
    Miss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eviction {
    pub tag: u64,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLineSnapshot {
    pub valid: bool,
    pub dirty: bool,
    pub tag: u64,
    pub recency_rank: Option<usize>,
}

/// One shared implementation of set-associative storage, reused by every
/// cache level and by the TLB. Only LRU (least recently used) replacement
/// is simulated, see this plan's Global Constraints.
#[derive(Debug)]
pub struct SetAssociativeCache {
    associativity: usize,
    sets: HashMap<u64, Vec<CacheLine>>,
    tick: u64,
    dirty_percent: u8,
    seed: u64,
}

/// A not-yet-touched way in a set counts as pre-loaded dirty if its roll
/// comes out under the dirty percent setting. This is always worked out
/// from the coordinates, never stored, so it gives the same answer whether
/// asked by a real access or by a display snapshot. `way` here is always
/// the slot a real insert would fill next, matching how `snapshot_set`
/// numbers empty ways, so both agree on which slots count as pre-loaded.
fn phantom_dirty_roll(dirty_percent: u8, seed: u64, set_index: u64, way: u64) -> bool {
    dirty_percent > 0 && seeded_random::percent_roll(seed, &[3, set_index, way]) < dirty_percent
}

/// The fake tag a pre-loaded dirty way shows or evicts. It's a random-
/// looking value that will basically never clash with a tag a real access
/// derives from an actual address.
fn phantom_tag(seed: u64, set_index: u64, way: u64) -> u64 {
    seeded_random::value(seed, &[4, set_index, way])
}

impl SetAssociativeCache {
    pub fn new(associativity: usize) -> Self {
        Self::with_dirty_seed(associativity, 0, 0)
    }

    /// A percentage of ways that a real access has never touched still
    /// count as already holding pre-existing dirty data, the same way a
    /// cache that didn't start out empty would be. So the first real insert
    /// into such a slot is a miss that evicts that pre-loaded line, rather
    /// than simply being added for free. The seed is decided once per
    /// simulation session (either the user's chosen value, or a freshly
    /// generated one, see `seeded_random::generate_seed`), and the caller
    /// derives a different one per level and side, so different caches in
    /// the same hierarchy don't all roll the same pattern.
    pub fn with_dirty_seed(associativity: usize, dirty_percent: u8, seed: u64) -> Self {
        Self {
            associativity,
            sets: HashMap::new(),
            tick: 0,
            dirty_percent,
            seed,
        }
    }

    pub fn lookup(&mut self, set_index: u64, tag: u64) -> LookupOutcome {
        self.tick += 1;
        if let Some(lines) = self.sets.get_mut(&set_index) {
            if let Some(line) = lines.iter_mut().find(|l| l.valid && l.tag == tag) {
                line.last_used = self.tick;
                return LookupOutcome::Hit;
            }
        }
        LookupOutcome::Miss
    }

    pub fn insert(&mut self, set_index: u64, tag: u64, dirty: bool) -> Option<Eviction> {
        self.tick += 1;
        let associativity = self.associativity;
        let dirty_percent = self.dirty_percent;
        let seed = self.seed;
        let lines = self.sets.entry(set_index).or_default();

        let evicted = if lines.len() >= associativity {
            let victim_pos = lines
                .iter()
                .enumerate()
                .min_by_key(|(_, l)| l.last_used)
                .map(|(i, _)| i)
                .expect("associativity is always > 0, so a full set always has a victim");
            let victim = lines.remove(victim_pos);
            Some(Eviction { tag: victim.tag, dirty: victim.dirty })
        } else {
            let way = lines.len() as u64;
            if phantom_dirty_roll(dirty_percent, seed, set_index, way) {
                Some(Eviction { tag: phantom_tag(seed, set_index, way), dirty: true })
            } else {
                None
            }
        };

        lines.push(CacheLine {
            tag,
            valid: true,
            dirty,
            last_used: self.tick,
        });

        evicted
    }

    pub fn mark_dirty(&mut self, set_index: u64, tag: u64) {
        if let Some(lines) = self.sets.get_mut(&set_index) {
            if let Some(line) = lines.iter_mut().find(|l| l.valid && l.tag == tag) {
                line.dirty = true;
            }
        }
    }

    pub fn associativity(&self) -> usize {
        self.associativity
    }

    /// Read-only: never changes the recency tracking, unlike `lookup`.
    /// Always returns exactly as many entries as the associativity; ways
    /// beyond how many lines this set currently holds are reported empty.
    pub fn snapshot_set(&self, set_index: u64) -> Vec<CacheLineSnapshot> {
        let empty: Vec<CacheLine> = Vec::new();
        let lines = self.sets.get(&set_index).unwrap_or(&empty);

        let mut recency_order: Vec<usize> = (0..lines.len()).collect();
        recency_order.sort_by_key(|&i| std::cmp::Reverse(lines[i].last_used));
        let mut rank_of: Vec<Option<usize>> = vec![None; lines.len()];
        for (rank, &i) in recency_order.iter().enumerate() {
            rank_of[i] = Some(rank);
        }

        (0..self.associativity)
            .map(|way| match lines.get(way) {
                Some(line) => CacheLineSnapshot { valid: true, dirty: line.dirty, tag: line.tag, recency_rank: rank_of[way] },
                None if phantom_dirty_roll(self.dirty_percent, self.seed, set_index, way as u64) => CacheLineSnapshot {
                    valid: true,
                    dirty: true,
                    tag: phantom_tag(self.seed, set_index, way as u64),
                    recency_rank: None,
                },
                None => CacheLineSnapshot { valid: false, dirty: false, tag: 0, recency_rank: None },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_on_empty_cache_is_a_miss() {
        let mut cache = SetAssociativeCache::new(2);
        assert_eq!(cache.lookup(0, 42), LookupOutcome::Miss);
    }

    #[test]
    fn lookup_after_insert_is_a_hit() {
        let mut cache = SetAssociativeCache::new(2);
        cache.insert(0, 42, false);
        assert_eq!(cache.lookup(0, 42), LookupOutcome::Hit);
    }

    #[test]
    fn different_tags_in_the_same_set_do_not_collide() {
        let mut cache = SetAssociativeCache::new(2);
        cache.insert(0, 1, false);
        cache.insert(0, 2, false);
        assert_eq!(cache.lookup(0, 1), LookupOutcome::Hit);
        assert_eq!(cache.lookup(0, 2), LookupOutcome::Hit);
    }

    #[test]
    fn different_sets_are_independent() {
        let mut cache = SetAssociativeCache::new(1);
        cache.insert(0, 42, false);
        assert_eq!(cache.lookup(1, 42), LookupOutcome::Miss);
    }

    #[test]
    fn inserting_into_a_full_set_evicts_the_least_recently_used_line() {
        let mut cache = SetAssociativeCache::new(2);
        cache.insert(0, 1, false);
        cache.insert(0, 2, false);
        // touch tag 1 so tag 2 becomes the least recently used one
        cache.lookup(0, 1);

        let evicted = cache.insert(0, 3, false);

        assert_eq!(evicted.unwrap().tag, 2);
        assert_eq!(cache.lookup(0, 1), LookupOutcome::Hit);
        assert_eq!(cache.lookup(0, 2), LookupOutcome::Miss);
        assert_eq!(cache.lookup(0, 3), LookupOutcome::Hit);
    }

    #[test]
    fn inserting_into_a_non_full_set_does_not_evict() {
        let mut cache = SetAssociativeCache::new(2);
        assert!(cache.insert(0, 1, false).is_none());
    }

    #[test]
    fn eviction_reports_whether_the_evicted_line_was_dirty() {
        let mut cache = SetAssociativeCache::new(1);
        cache.insert(0, 1, true);
        let evicted = cache.insert(0, 2, false).unwrap();
        assert_eq!(evicted.tag, 1);
        assert!(evicted.dirty);
    }

    #[test]
    fn mark_dirty_sets_the_dirty_bit_on_a_resident_line() {
        let mut cache = SetAssociativeCache::new(1);
        cache.insert(0, 1, false);
        cache.mark_dirty(0, 1);
        // the simplest way to check the dirty bit is to evict the line and look at the flag
        let evicted = cache.insert(0, 2, false).unwrap();
        assert!(evicted.dirty);
    }

    #[test]
    fn mark_dirty_on_a_tag_not_resident_is_a_no_op() {
        let mut cache = SetAssociativeCache::new(1);
        cache.mark_dirty(0, 999); // must not crash
    }

    #[test]
    fn snapshot_set_of_an_untouched_set_is_all_empty_ways() {
        let cache = SetAssociativeCache::new(2);
        let snap = cache.snapshot_set(0);
        assert_eq!(snap.len(), 2);
        assert!(snap.iter().all(|w| !w.valid && w.tag == 0 && w.recency_rank.is_none()));
    }

    #[test]
    fn snapshot_set_reports_valid_ways_with_tag_and_dirty() {
        let mut cache = SetAssociativeCache::new(2);
        cache.insert(0, 42, true);
        let snap = cache.snapshot_set(0);
        assert_eq!(snap.len(), 2);
        assert!(snap[0].valid);
        assert_eq!(snap[0].tag, 42);
        assert!(snap[0].dirty);
        assert!(!snap[1].valid);
    }

    #[test]
    fn snapshot_set_ranks_recency_with_zero_as_most_recent() {
        let mut cache = SetAssociativeCache::new(2);
        cache.insert(0, 1, false);
        cache.insert(0, 2, false);
        cache.lookup(0, 1); // touch tag 1 so it becomes the most recently used one

        let snap = cache.snapshot_set(0);
        let rank_of = |tag: u64| snap.iter().find(|w| w.valid && w.tag == tag).unwrap().recency_rank.unwrap();
        assert_eq!(rank_of(1), 0);
        assert_eq!(rank_of(2), 1);
    }

    #[test]
    fn snapshot_set_does_not_mutate_recency() {
        let mut cache = SetAssociativeCache::new(1);
        cache.insert(0, 1, false);
        cache.snapshot_set(0); // must not affect recency tracking
        cache.insert(0, 2, false); // eviction order wouldn't actually change here even if it did, but the real safeguard is that this function only reads, it can't modify anything
        let evicted = cache.insert(0, 3, false);
        assert_eq!(evicted.unwrap().tag, 2);
    }

    #[test]
    fn associativity_returns_the_configured_value() {
        let cache = SetAssociativeCache::new(4);
        assert_eq!(cache.associativity(), 4);
    }

    #[test]
    fn zero_dirty_percent_never_phantom_seeds_a_set() {
        let cache = SetAssociativeCache::with_dirty_seed(2, 0, 1);
        let snap = cache.snapshot_set(0);
        assert!(snap.iter().all(|w| !w.valid));
    }

    #[test]
    fn full_dirty_percent_phantom_seeds_every_way_of_an_untouched_set() {
        let cache = SetAssociativeCache::with_dirty_seed(2, 100, 1);
        let snap = cache.snapshot_set(0);
        assert_eq!(snap.len(), 2);
        assert!(snap.iter().all(|w| w.valid && w.dirty));
        // Different way slots roll different fake tags.
        assert_ne!(snap[0].tag, snap[1].tag);
    }

    #[test]
    fn phantom_seeding_is_deterministic_across_separate_instances_with_the_same_seed() {
        let a = SetAssociativeCache::with_dirty_seed(2, 100, 7);
        let b = SetAssociativeCache::with_dirty_seed(2, 100, 7);
        assert_eq!(a.snapshot_set(3)[1].tag, b.snapshot_set(3)[1].tag);
    }

    #[test]
    fn a_different_seed_rolls_different_phantom_tags() {
        let a = SetAssociativeCache::with_dirty_seed(2, 100, 1);
        let b = SetAssociativeCache::with_dirty_seed(2, 100, 2);
        assert_ne!(a.snapshot_set(0)[0].tag, b.snapshot_set(0)[0].tag);
    }

    #[test]
    fn a_real_insert_into_a_phantom_dirty_slot_evicts_it_instead_of_freely_appending() {
        let mut cache = SetAssociativeCache::with_dirty_seed(2, 100, 1);
        let phantom_tag = cache.snapshot_set(0)[0].tag;

        let evicted = cache.insert(0, 999, false);

        assert_eq!(evicted, Some(Eviction { tag: phantom_tag, dirty: true }));
        // The set now holds one real line; only the still-untouched way
        // slot rolls as pre-loaded dirty.
        let snap = cache.snapshot_set(0);
        assert_eq!(snap[0].tag, 999);
        assert!(!snap[0].dirty);
        assert!(snap[1].valid && snap[1].dirty);
    }

    #[test]
    fn once_a_slot_holds_a_real_line_it_no_longer_rolls_as_phantom() {
        let mut cache = SetAssociativeCache::with_dirty_seed(1, 100, 1);
        cache.insert(0, 1, false);
        // The only way is now real; a second insert must evict the real
        // line through normal LRU, not roll a new pre-loaded dirty line for
        // the same slot.
        let evicted = cache.insert(0, 2, false);
        assert_eq!(evicted, Some(Eviction { tag: 1, dirty: false }));
    }
}
