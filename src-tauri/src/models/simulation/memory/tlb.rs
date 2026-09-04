use std::collections::HashMap;

use serde::Serialize;

use super::replacement;

#[derive(Debug, Clone, Copy)]
struct TlbEntry {
    vpn: u64,
    ppn: u64,
    last_used: u64,
    inserted_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TlbEntrySnapshot {
    pub valid: bool,
    pub vpn: u64,
    pub ppn: u64,
    pub order_value: Option<u64>,
}

/// A store just for VPN to PPN translations, simpler than the cache since a translation is either known or it is not.
/// Simulates LRU, FIFO, and Random eviction, never LFU, and PLRU stays decorative and behaves like LRU.
pub struct Tlb {
    num_sets: u64,
    associativity: usize,
    sets: HashMap<u64, Vec<TlbEntry>>,
    tick: u64,
    policy: replacement::Policy,
    seed: u64,
}

impl Tlb {
    pub fn new(entries: u32, associativity: u32) -> Self {
        Self::with_policy_and_seed(entries, associativity, replacement::Policy::Lru, 0)
    }

    pub fn with_policy_and_seed(entries: u32, associativity: u32, policy: replacement::Policy, seed: u64) -> Self {
        let associativity = associativity.max(1) as usize;
        let num_sets = ((entries.max(1) as u64) / associativity as u64).max(1);
        Self { num_sets, associativity, sets: HashMap::new(), tick: 0, policy, seed }
    }

    fn set_index(&self, vpn: u64) -> u64 {
        vpn % self.num_sets
    }

    pub fn lookup(&mut self, vpn: u64) -> Option<u64> {
        self.tick += 1;
        let tick = self.tick;
        let set_index = self.set_index(vpn);
        let entries = self.sets.get_mut(&set_index)?;
        let entry = entries.iter_mut().find(|e| e.vpn == vpn)?;
        entry.last_used = tick;
        Some(entry.ppn)
    }

    pub fn insert(&mut self, vpn: u64, ppn: u64) {
        self.tick += 1;
        let tick = self.tick;
        let associativity = self.associativity;
        let policy = self.policy;
        let seed = self.seed;
        let set_index = self.set_index(vpn);
        let entries = self.sets.entry(set_index).or_default();
        if let Some(existing) = entries.iter_mut().find(|e| e.vpn == vpn) {
            existing.ppn = ppn;
            existing.last_used = tick;
            return;
        }
        if entries.len() >= associativity {
            let candidates: Vec<replacement::Candidate> = entries
                .iter()
                .map(|e| replacement::Candidate { last_used: e.last_used, inserted_at: e.inserted_at, frequency: 0 })
                .collect();
            let victim = replacement::pick_victim(policy, &candidates, seed, &[10, set_index, tick]);
            entries.remove(victim);
        }
        entries.push(TlbEntry { vpn, ppn, last_used: tick, inserted_at: tick });
    }

    /// Removes any entry for this vpn, doing nothing if there is none.
    /// Needed whenever its frame gets handed to a different page, so a later lookup never gets the wrong answer.
    pub fn invalidate(&mut self, vpn: u64) {
        let set_index = self.set_index(vpn);
        if let Some(entries) = self.sets.get_mut(&set_index) {
            entries.retain(|e| e.vpn != vpn);
        }
    }

    pub fn num_sets(&self) -> u64 {
        self.num_sets
    }

    pub fn associativity(&self) -> usize {
        self.associativity
    }

    pub fn snapshot_set(&self, set_index: u64) -> Vec<TlbEntrySnapshot> {
        let empty: Vec<TlbEntry> = Vec::new();
        let entries = self.sets.get(&set_index).unwrap_or(&empty);
        let order_of = match self.policy {
            replacement::Policy::Random => vec![None; entries.len()],
            replacement::Policy::Fifo => replacement::rank_by_key(entries, |e| e.inserted_at),
            // Lfu is never selected for a TLB, this case only exists to keep the match complete.
            replacement::Policy::Lru | replacement::Policy::Lfu => replacement::rank_by_key(entries, |e| e.last_used),
        };
        (0..self.associativity)
            .map(|way| match entries.get(way) {
                Some(e) => TlbEntrySnapshot { valid: true, vpn: e.vpn, ppn: e.ppn, order_value: order_of[way] },
                None => TlbEntrySnapshot { valid: false, vpn: 0, ppn: 0, order_value: None },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_tlb_misses_on_every_lookup() {
        let mut tlb = Tlb::new(4, 2);
        assert_eq!(tlb.lookup(1), None);
    }

    #[test]
    fn lookup_after_insert_returns_the_ppn() {
        let mut tlb = Tlb::new(4, 2);
        tlb.insert(1, 100);
        assert_eq!(tlb.lookup(1), Some(100));
    }

    #[test]
    fn invalidate_removes_the_entry_so_the_next_lookup_misses() {
        let mut tlb = Tlb::new(4, 2);
        tlb.insert(1, 100);
        assert_eq!(tlb.lookup(1), Some(100));
        tlb.invalidate(1);
        assert_eq!(tlb.lookup(1), None);
    }

    #[test]
    fn invalidating_an_absent_vpn_is_a_no_op() {
        let mut tlb = Tlb::new(4, 2);
        tlb.insert(2, 20);
        tlb.invalidate(1); // never inserted, and lands in the same set as vpn 2 could
        assert_eq!(tlb.lookup(2), Some(20), "invalidating an unrelated vpn must not disturb an existing entry");
    }

    #[test]
    fn invalidate_only_removes_the_matching_vpn_within_its_set() {
        let mut tlb = Tlb::new(4, 2); // 2 sets, associativity 2
        tlb.insert(0, 10);
        tlb.insert(2, 20); // shares set 0 with vpn 0 (0 % 2 == 2 % 2 == 0)
        tlb.invalidate(0);
        assert_eq!(tlb.lookup(0), None);
        assert_eq!(tlb.lookup(2), Some(20), "a sibling entry in the same set must survive");
    }

    #[test]
    fn entries_equals_two_and_associativity_one_makes_two_sets() {
        let tlb = Tlb::new(2, 1);
        assert_eq!(tlb.num_sets(), 2);
        assert_eq!(tlb.associativity(), 1);
    }

    #[test]
    fn different_vpns_landing_in_the_same_set_do_not_collide_within_associativity() {
        let mut tlb = Tlb::new(4, 2); // 2 sets, associativity 2
        // vpn 0 and vpn 2 both map to set 0.
        tlb.insert(0, 10);
        tlb.insert(2, 20);
        assert_eq!(tlb.lookup(0), Some(10));
        assert_eq!(tlb.lookup(2), Some(20));
    }

    #[test]
    fn inserting_past_associativity_evicts_the_least_recently_used_entry() {
        let mut tlb = Tlb::new(4, 2); // 2 sets, associativity 2, so 1 set holds 2 ways
        // Force everything into set 0 by using multiples of the set count.
        tlb.insert(0, 10);
        tlb.insert(2, 20);
        tlb.lookup(0); // touch vpn 0 so vpn 2 becomes the least recently used
        tlb.insert(4, 40); // also lands in set 0, set is full, evicts vpn 2

        assert_eq!(tlb.lookup(0), Some(10));
        assert_eq!(tlb.lookup(2), None);
        assert_eq!(tlb.lookup(4), Some(40));
    }

    #[test]
    fn inserting_an_already_present_vpn_updates_it_in_place_instead_of_evicting() {
        let mut tlb = Tlb::new(4, 1);
        tlb.insert(1, 100);
        tlb.insert(1, 200);
        assert_eq!(tlb.lookup(1), Some(200));
    }

    #[test]
    fn snapshot_set_of_an_untouched_set_is_all_empty_ways() {
        let tlb = Tlb::new(4, 2);
        let snap = tlb.snapshot_set(0);
        assert_eq!(snap.len(), 2);
        assert!(snap.iter().all(|w| !w.valid && w.order_value.is_none()));
    }

    #[test]
    fn snapshot_set_reports_valid_ways_with_vpn_and_ppn() {
        let mut tlb = Tlb::new(4, 2);
        tlb.insert(0, 100);
        let snap = tlb.snapshot_set(0);
        assert!(snap[0].valid);
        assert_eq!(snap[0].vpn, 0);
        assert_eq!(snap[0].ppn, 100);
        assert!(!snap[1].valid);
    }

    #[test]
    fn snapshot_set_ranks_recency_with_zero_as_most_recent() {
        let mut tlb = Tlb::new(4, 2);
        tlb.insert(0, 10);
        tlb.insert(2, 20);
        tlb.lookup(0);
        let snap = tlb.snapshot_set(0);
        let rank_of = |vpn: u64| snap.iter().find(|w| w.valid && w.vpn == vpn).unwrap().order_value.unwrap();
        assert_eq!(rank_of(0), 0);
        assert_eq!(rank_of(2), 1);
    }

    #[test]
    fn fifo_evicts_the_first_inserted_entry_even_after_a_later_hit() {
        let mut tlb = Tlb::with_policy_and_seed(4, 2, replacement::Policy::Fifo, 0); // 2 sets, associativity 2
        tlb.insert(0, 10);
        tlb.insert(2, 20); // shares set 0 with vpn 0 (0 % 2 == 2 % 2 == 0)
        tlb.lookup(0); // a hit on vpn 0 must not save it from FIFO eviction
        tlb.insert(4, 40); // also lands in set 0, forces an eviction

        assert_eq!(tlb.lookup(0), None, "FIFO evicts by insertion order, a hit does not reorder anything");
        assert_eq!(tlb.lookup(2), Some(20));
        assert_eq!(tlb.lookup(4), Some(40));
    }

    #[test]
    fn random_eviction_is_deterministic_for_the_same_seed() {
        let mut a = Tlb::with_policy_and_seed(4, 2, replacement::Policy::Random, 42);
        a.insert(0, 10);
        a.insert(2, 20);
        a.insert(4, 40);

        let mut b = Tlb::with_policy_and_seed(4, 2, replacement::Policy::Random, 42);
        b.insert(0, 10);
        b.insert(2, 20);
        b.insert(4, 40);

        assert_eq!(a.lookup(0), b.lookup(0));
        assert_eq!(a.lookup(2), b.lookup(2));
    }

    #[test]
    fn snapshot_set_reports_no_order_value_under_random_even_for_a_valid_entry() {
        let mut tlb = Tlb::with_policy_and_seed(4, 1, replacement::Policy::Random, 0);
        tlb.insert(0, 10);
        let snap = tlb.snapshot_set(0);
        assert!(snap[0].valid);
        assert_eq!(snap[0].order_value, None);
    }
}
