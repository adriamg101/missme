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
}
