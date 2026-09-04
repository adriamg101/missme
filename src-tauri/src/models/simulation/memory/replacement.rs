use super::seeded_random;

/// The replacement policy actually simulated. PLRU has no variant here, both config types map it to Lru below.
/// It stays decorative, exactly as before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Lru,
    Fifo,
    Random,
    Lfu,
}

/// One candidate an eviction can choose between. Frequency only matters for Lfu.
/// A caller that never offers Lfu, like the TLB, can always pass 0 for it.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub last_used: u64,
    pub inserted_at: u64,
    pub frequency: u64,
}

/// Picks which candidate to evict when a set is full.
/// The seed and context feed the Random policy the same way every other randomized part of the simulator works, so it stays reproducible.
pub fn pick_victim(policy: Policy, candidates: &[Candidate], seed: u64, context: &[u64]) -> usize {
    match policy {
        Policy::Lru => argmin(candidates, |c| c.last_used),
        Policy::Fifo => argmin(candidates, |c| c.inserted_at),
        Policy::Lfu => argmin(candidates, |c| (c.frequency, c.last_used)),
        Policy::Random => (seeded_random::value(seed, context) as usize) % candidates.len(),
    }
}

fn argmin<K: Ord>(candidates: &[Candidate], key: impl Fn(&Candidate) -> K) -> usize {
    candidates
        .iter()
        .enumerate()
        .min_by_key(|(_, c)| key(c))
        .map(|(i, _)| i)
        .expect("pick_victim is only ever called with a non-empty, full set of candidates")
}

/// Ranks a list of items for display, 0 is safest from eviction and higher numbers are closer to being evicted.
/// Used for LRU and FIFO's inspector column, LFU and Random show something different and never call this.
pub fn rank_by_key<T>(items: &[T], key: impl Fn(&T) -> u64) -> Vec<Option<u64>> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(key(&items[i])));
    let mut rank_of = vec![None; items.len()];
    for (rank, &i) in order.iter().enumerate() {
        rank_of[i] = Some(rank as u64);
    }
    rank_of
}

impl From<super::config::ReplacementPolicy> for Policy {
    fn from(p: super::config::ReplacementPolicy) -> Self {
        use super::config::ReplacementPolicy::*;
        match p {
            Lru | Plru => Policy::Lru,
            Fifo => Policy::Fifo,
            Random => Policy::Random,
            Lfu => Policy::Lfu,
        }
    }
}

impl From<super::config::TlbReplacementPolicy> for Policy {
    fn from(p: super::config::TlbReplacementPolicy) -> Self {
        use super::config::TlbReplacementPolicy::*;
        match p {
            Lru | Plru => Policy::Lru,
            Fifo => Policy::Fifo,
            Random => Policy::Random,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(last_used: u64, inserted_at: u64, frequency: u64) -> Candidate {
        Candidate { last_used, inserted_at, frequency }
    }

    #[test]
    fn lru_picks_the_candidate_with_the_smallest_last_used() {
        let candidates = [candidate(5, 1, 1), candidate(2, 2, 1), candidate(9, 3, 1)];
        assert_eq!(pick_victim(Policy::Lru, &candidates, 0, &[]), 1);
    }

    #[test]
    fn fifo_picks_the_candidate_with_the_smallest_inserted_at_ignoring_last_used() {
        let candidates = [candidate(100, 3, 1), candidate(1, 1, 1), candidate(50, 2, 1)];
        assert_eq!(pick_victim(Policy::Fifo, &candidates, 0, &[]), 1);
    }

    #[test]
    fn lfu_picks_the_candidate_with_the_smallest_frequency() {
        let candidates = [candidate(1, 1, 5), candidate(2, 2, 1), candidate(3, 3, 9)];
        assert_eq!(pick_victim(Policy::Lfu, &candidates, 0, &[]), 1);
    }

    #[test]
    fn lfu_breaks_a_frequency_tie_by_the_smallest_last_used() {
        let candidates = [candidate(10, 1, 1), candidate(5, 2, 1)];
        assert_eq!(pick_victim(Policy::Lfu, &candidates, 0, &[]), 1);
    }

    #[test]
    fn random_is_deterministic_for_the_same_seed_and_context() {
        let candidates = [candidate(1, 1, 1), candidate(2, 2, 1), candidate(3, 3, 1)];
        let a = pick_victim(Policy::Random, &candidates, 42, &[7, 1]);
        let b = pick_victim(Policy::Random, &candidates, 42, &[7, 1]);
        assert_eq!(a, b);
    }

    #[test]
    fn random_always_returns_a_valid_index() {
        let candidates = [candidate(1, 1, 1), candidate(2, 2, 1)];
        for seed in 0..50u64 {
            let victim = pick_victim(Policy::Random, &candidates, seed, &[7, seed]);
            assert!(victim < candidates.len());
        }
    }

    #[test]
    fn rank_by_key_ranks_zero_as_the_highest_key() {
        let items = [10u64, 30u64, 20u64];
        let ranks = rank_by_key(&items, |&x| x);
        assert_eq!(ranks[1], Some(0)); // 30 is highest, so it's rank 0
        assert_eq!(ranks[2], Some(1)); // 20 is next
        assert_eq!(ranks[0], Some(2)); // 10 is lowest, so it's the last rank
    }

    #[test]
    fn cache_replacement_policy_plru_maps_to_lru() {
        assert_eq!(Policy::from(super::super::config::ReplacementPolicy::Plru), Policy::Lru);
    }

    #[test]
    fn tlb_replacement_policy_plru_maps_to_lru() {
        assert_eq!(Policy::from(super::super::config::TlbReplacementPolicy::Plru), Policy::Lru);
    }

    #[test]
    fn every_cache_replacement_policy_maps_to_the_matching_internal_policy() {
        use super::super::config::ReplacementPolicy;
        assert_eq!(Policy::from(ReplacementPolicy::Lru), Policy::Lru);
        assert_eq!(Policy::from(ReplacementPolicy::Fifo), Policy::Fifo);
        assert_eq!(Policy::from(ReplacementPolicy::Random), Policy::Random);
        assert_eq!(Policy::from(ReplacementPolicy::Lfu), Policy::Lfu);
    }

    #[test]
    fn every_tlb_replacement_policy_maps_to_the_matching_internal_policy() {
        use super::super::config::TlbReplacementPolicy;
        assert_eq!(Policy::from(TlbReplacementPolicy::Lru), Policy::Lru);
        assert_eq!(Policy::from(TlbReplacementPolicy::Fifo), Policy::Fifo);
        assert_eq!(Policy::from(TlbReplacementPolicy::Random), Policy::Random);
    }
}
