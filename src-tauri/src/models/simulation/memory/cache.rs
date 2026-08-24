use std::collections::HashMap;

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

/// A single shared implementation of set-associative storage, reused by
/// every cache level and by the TLB. Only LRU replacement is simulated —
/// see this plan's Global Constraints.
#[derive(Debug)]
pub struct SetAssociativeCache {
    associativity: usize,
    sets: HashMap<u64, Vec<CacheLine>>,
    tick: u64,
}

impl SetAssociativeCache {
    pub fn new(associativity: usize) -> Self {
        Self {
            associativity,
            sets: HashMap::new(),
            tick: 0,
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
            None
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

    /// Read-only: never touches `last_used`/`tick`, unlike `lookup`. Always
    /// returns exactly `associativity` entries; ways beyond how many lines
    /// this set currently holds are reported empty.
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
        // touch tag 1 so tag 2 becomes the least recently used
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
        // re-insert-free way to observe dirty: evict it and check the flag
        let evicted = cache.insert(0, 2, false).unwrap();
        assert!(evicted.dirty);
    }

    #[test]
    fn mark_dirty_on_a_tag_not_resident_is_a_no_op() {
        let mut cache = SetAssociativeCache::new(1);
        cache.mark_dirty(0, 999); // must not panic
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
        cache.lookup(0, 1); // touch tag 1 so it becomes most recently used

        let snap = cache.snapshot_set(0);
        let rank_of = |tag: u64| snap.iter().find(|w| w.valid && w.tag == tag).unwrap().recency_rank.unwrap();
        assert_eq!(rank_of(1), 0);
        assert_eq!(rank_of(2), 1);
    }

    #[test]
    fn snapshot_set_does_not_mutate_recency() {
        let mut cache = SetAssociativeCache::new(1);
        cache.insert(0, 1, false);
        cache.snapshot_set(0); // must not touch last_used/tick
        cache.insert(0, 2, false); // if snapshot_set bumped tick, eviction order would be unaffected here anyway; the real guard is the compiler: snapshot_set takes &self
        let evicted = cache.insert(0, 3, false);
        assert_eq!(evicted.unwrap().tag, 2);
    }

    #[test]
    fn associativity_returns_the_configured_value() {
        let cache = SetAssociativeCache::new(4);
        assert_eq!(cache.associativity(), 4);
    }
}
