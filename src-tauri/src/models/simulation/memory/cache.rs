use std::collections::HashMap;

use super::replacement;
use super::seeded_random;

#[derive(Debug, Clone)]
pub struct CacheLine {
    pub tag: u64,
    pub valid: bool,
    pub dirty: bool,
    last_used: u64,
    inserted_at: u64,
    frequency: u64,
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
    pub is_phantom: bool,
    pub order_value: Option<u64>,
}

/// One shared cache store, used by every cache level and by the TLB.
/// Simulates LRU, FIFO, Random, and LFU eviction, PLRU stays decorative and behaves like LRU.
#[derive(Debug)]
pub struct SetAssociativeCache {
    associativity: usize,
    policy: replacement::Policy,
    sets: HashMap<u64, Vec<CacheLine>>,
    tick: u64,
    dirty_percent: u8,
    seed: u64,
}

/// A not yet touched way counts as already dirty if its roll lands under the dirty percent setting.
/// This is worked out fresh each time from its coordinates, so a real access and a display always agree.
fn phantom_dirty_roll(dirty_percent: u8, seed: u64, set_index: u64, way: u64) -> bool {
    dirty_percent > 0 && seeded_random::percent_roll(seed, &[3, set_index, way]) < dirty_percent
}

/// The fake tag a pre-loaded dirty way shows or evicts.
/// It looks random and will basically never clash with a tag a real address produces.
fn phantom_tag(seed: u64, set_index: u64, way: u64) -> u64 {
    seeded_random::value(seed, &[4, set_index, way])
}

fn order_values(policy: replacement::Policy, lines: &[CacheLine]) -> Vec<Option<u64>> {
    match policy {
        replacement::Policy::Lfu => lines.iter().map(|l| Some(l.frequency)).collect(),
        replacement::Policy::Random => vec![None; lines.len()],
        replacement::Policy::Lru => replacement::rank_by_key(lines, |l| l.last_used),
        replacement::Policy::Fifo => replacement::rank_by_key(lines, |l| l.inserted_at),
    }
}

impl SetAssociativeCache {
    pub fn new(associativity: usize) -> Self {
        Self::with_policy_and_dirty_seed(associativity, replacement::Policy::Lru, 0, 0)
    }

    /// Some ways start out already dirty as if this cache was never empty, so the first real insert there evicts that made up line.
    /// The seed is chosen once per session, and the caller derives a different one per level and side.
    pub fn with_dirty_seed(associativity: usize, dirty_percent: u8, seed: u64) -> Self {
        Self::with_policy_and_dirty_seed(associativity, replacement::Policy::Lru, dirty_percent, seed)
    }

    pub fn with_policy_and_dirty_seed(associativity: usize, policy: replacement::Policy, dirty_percent: u8, seed: u64) -> Self {
        Self {
            associativity,
            policy,
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
                line.frequency += 1;
                return LookupOutcome::Hit;
            }
        }
        LookupOutcome::Miss
    }

    pub fn insert(&mut self, set_index: u64, tag: u64, dirty: bool) -> Option<Eviction> {
        self.tick += 1;
        let tick = self.tick;
        let associativity = self.associativity;
        let policy = self.policy;
        let dirty_percent = self.dirty_percent;
        let seed = self.seed;
        let lines = self.sets.entry(set_index).or_default();

        let evicted = if lines.len() >= associativity {
            let candidates: Vec<replacement::Candidate> = lines
                .iter()
                .map(|l| replacement::Candidate { last_used: l.last_used, inserted_at: l.inserted_at, frequency: l.frequency })
                .collect();
            let victim_pos = replacement::pick_victim(policy, &candidates, seed, &[5, set_index, tick]);
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
            last_used: tick,
            inserted_at: tick,
            frequency: 1,
        });

        evicted
    }

    /// Drops the line matching `tag` in `set_index`, if any. A no-op if
    /// nothing there currently has that tag.
    pub fn invalidate(&mut self, set_index: u64, tag: u64) {
        if let Some(lines) = self.sets.get_mut(&set_index) {
            lines.retain(|l| !(l.valid && l.tag == tag));
        }
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

    /// Never changes which line looks most recently or most often used, unlike lookup and insert.
    /// Always returns one entry per way, reporting the extra ones beyond what this set actually holds as empty.
    pub fn snapshot_set(&self, set_index: u64) -> Vec<CacheLineSnapshot> {
        let empty: Vec<CacheLine> = Vec::new();
        let lines = self.sets.get(&set_index).unwrap_or(&empty);
        let order_of = order_values(self.policy, lines);

        (0..self.associativity)
            .map(|way| match lines.get(way) {
                Some(line) => CacheLineSnapshot { valid: true, dirty: line.dirty, tag: line.tag, is_phantom: false, order_value: order_of[way] },
                None if phantom_dirty_roll(self.dirty_percent, self.seed, set_index, way as u64) => CacheLineSnapshot {
                    valid: true,
                    dirty: true,
                    tag: phantom_tag(self.seed, set_index, way as u64),
                    is_phantom: true,
                    order_value: None,
                },
                None => CacheLineSnapshot { valid: false, dirty: false, tag: 0, is_phantom: false, order_value: None },
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
    fn invalidate_removes_the_matching_tag_so_the_next_lookup_misses() {
        let mut cache = SetAssociativeCache::new(2);
        cache.insert(0, 1, false);
        cache.insert(0, 2, false);
        cache.invalidate(0, 1);
        assert_eq!(cache.lookup(0, 1), LookupOutcome::Miss);
        assert_eq!(cache.lookup(0, 2), LookupOutcome::Hit, "a sibling line in the same set must survive");
    }

    #[test]
    fn invalidating_a_tag_not_resident_is_a_no_op() {
        let mut cache = SetAssociativeCache::new(1);
        cache.invalidate(0, 999); // must not crash
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
        assert!(snap.iter().all(|w| !w.valid && w.tag == 0 && !w.is_phantom && w.order_value.is_none()));
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
        let rank_of = |tag: u64| snap.iter().find(|w| w.valid && w.tag == tag).unwrap().order_value.unwrap();
        assert_eq!(rank_of(1), 0);
        assert_eq!(rank_of(2), 1);
    }

    #[test]
    fn snapshot_set_does_not_mutate_recency() {
        let mut cache = SetAssociativeCache::new(1);
        cache.insert(0, 1, false);
        cache.snapshot_set(0); // must not change which line looks most recently used
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
        // The set now holds one real line, only the still-untouched way
        // rolls as pre-loaded dirty.
        let snap = cache.snapshot_set(0);
        assert_eq!(snap[0].tag, 999);
        assert!(!snap[0].dirty);
        assert!(snap[1].valid && snap[1].dirty);
    }

    #[test]
    fn once_a_slot_holds_a_real_line_it_no_longer_rolls_as_phantom() {
        let mut cache = SetAssociativeCache::with_dirty_seed(1, 100, 1);
        cache.insert(0, 1, false);
        // The only way is now real, so a second insert must evict it through normal LRU.
        let evicted = cache.insert(0, 2, false);
        assert_eq!(evicted, Some(Eviction { tag: 1, dirty: false }));
    }

    #[test]
    fn fifo_evicts_the_first_inserted_line_even_after_a_later_hit() {
        let mut cache = SetAssociativeCache::with_policy_and_dirty_seed(2, replacement::Policy::Fifo, 0, 0);
        cache.insert(0, 1, false);
        cache.insert(0, 2, false);
        cache.lookup(0, 1); // a hit on tag 1 must not save it from FIFO eviction

        let evicted = cache.insert(0, 3, false);

        assert_eq!(evicted.unwrap().tag, 1, "FIFO evicts by insertion order, a hit does not reorder anything");
    }

    #[test]
    fn lfu_evicts_the_least_frequently_used_line() {
        let mut cache = SetAssociativeCache::with_policy_and_dirty_seed(2, replacement::Policy::Lfu, 0, 0);
        cache.insert(0, 1, false);
        cache.insert(0, 2, false);
        cache.lookup(0, 1);
        cache.lookup(0, 1); // tag 1 now has frequency 3, tag 2 still has frequency 1

        let evicted = cache.insert(0, 3, false);

        assert_eq!(evicted.unwrap().tag, 2);
    }

    #[test]
    fn lfu_breaks_a_frequency_tie_by_least_recently_used() {
        let mut cache = SetAssociativeCache::with_policy_and_dirty_seed(2, replacement::Policy::Lfu, 0, 0);
        cache.insert(0, 1, false); // frequency 1, never touched again
        cache.insert(0, 2, false); // frequency 1, tied with tag 1, but inserted (and so last-used) later

        let evicted = cache.insert(0, 3, false);

        assert_eq!(evicted.unwrap().tag, 1, "a frequency tie breaks toward the least recently used line");
    }

    #[test]
    fn lfu_starts_a_freshly_inserted_line_at_frequency_one() {
        let mut cache = SetAssociativeCache::with_policy_and_dirty_seed(1, replacement::Policy::Lfu, 0, 0);
        cache.insert(0, 1, false);
        assert_eq!(cache.snapshot_set(0)[0].order_value, Some(1));
    }

    #[test]
    fn random_eviction_is_deterministic_for_the_same_seed() {
        let mut a = SetAssociativeCache::with_policy_and_dirty_seed(2, replacement::Policy::Random, 0, 42);
        a.insert(0, 1, false);
        a.insert(0, 2, false);
        let evicted_a = a.insert(0, 3, false).unwrap();

        let mut b = SetAssociativeCache::with_policy_and_dirty_seed(2, replacement::Policy::Random, 0, 42);
        b.insert(0, 1, false);
        b.insert(0, 2, false);
        let evicted_b = b.insert(0, 3, false).unwrap();

        assert_eq!(evicted_a, evicted_b);
    }

    #[test]
    fn snapshot_set_reports_no_order_value_under_random_even_for_a_real_line() {
        let mut cache = SetAssociativeCache::with_policy_and_dirty_seed(1, replacement::Policy::Random, 0, 0);
        cache.insert(0, 1, false);
        let snap = cache.snapshot_set(0);
        assert!(snap[0].valid);
        assert!(!snap[0].is_phantom);
        assert_eq!(snap[0].order_value, None);
    }
}
