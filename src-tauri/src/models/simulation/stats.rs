use std::collections::HashMap;

use serde::Serialize;

use super::memory::hierarchy::{AccessKind, LevelOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Unified,
    Instruction,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Tlb,
    Cache { level_index: usize, side: Side },
    Ram,
    Disk,
}

#[derive(Debug, Clone, Copy, Default)]
struct BucketStats {
    accesses: u64,
    hits: u64,
    misses: u64,
    cycles: u64,
}

pub struct StatsAccumulator {
    instructions_completed: u64,
    total_cycles: u64,
    per_bucket: HashMap<Bucket, BucketStats>,
    top_of_chain_instruction_accesses: u64,
    top_of_chain_data_accesses: u64,
}

impl StatsAccumulator {
    pub fn new() -> Self {
        Self {
            instructions_completed: 0,
            total_cycles: 0,
            per_bucket: HashMap::new(),
            top_of_chain_instruction_accesses: 0,
            top_of_chain_data_accesses: 0,
        }
    }

    // Called for every memory step, whether or not it carries hit or miss meaning for this bucket.
    // Grows the cycle total and, when this is the very first step of its side, the matching global count.
    pub fn record_access(&mut self, bucket: Bucket, access_kind: AccessKind, cost_cycles: u64, is_top_of_chain: bool) {
        self.total_cycles += cost_cycles;
        let stats = self.per_bucket.entry(bucket).or_default();
        stats.accesses += 1;
        stats.cycles += cost_cycles;
        if is_top_of_chain {
            match access_kind {
                AccessKind::Fetch => self.top_of_chain_instruction_accesses += 1,
                AccessKind::Load | AccessKind::Store => self.top_of_chain_data_accesses += 1,
            }
        }
    }

    // Called only for steps that carry real hit or miss meaning for this bucket.
    pub fn record_outcome(&mut self, bucket: Bucket, outcome: LevelOutcome) {
        let stats = self.per_bucket.entry(bucket).or_default();
        match outcome {
            LevelOutcome::Hit => stats.hits += 1,
            LevelOutcome::Miss => stats.misses += 1,
        }
    }

    pub fn record_instruction_complete(&mut self) {
        self.instructions_completed += 1;
        self.total_cycles += 1;
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        let total_memory_accesses = self.top_of_chain_instruction_accesses + self.top_of_chain_data_accesses;
        let memory_cycles = self.total_cycles.saturating_sub(self.instructions_completed);
        let amat = if total_memory_accesses > 0 { memory_cycles as f64 / total_memory_accesses as f64 } else { 0.0 };
        let cpi = if self.instructions_completed > 0 { self.total_cycles as f64 / self.instructions_completed as f64 } else { 0.0 };
        let mut buckets: Vec<BucketSnapshot> = self
            .per_bucket
            .iter()
            .map(|(bucket, stats)| bucket_snapshot(*bucket, stats, self.instructions_completed))
            .collect();
        buckets.sort_by_key(bucket_sort_key);

        StatsSnapshot {
            instructions_completed: self.instructions_completed,
            total_memory_accesses,
            top_of_chain_instruction_accesses: self.top_of_chain_instruction_accesses,
            top_of_chain_data_accesses: self.top_of_chain_data_accesses,
            total_cycles: self.total_cycles,
            amat,
            cpi,
            buckets,
        }
    }
}

// Global, TLB, then cache levels in order, then RAM, then disk. Matches the order the frontend's tabs already use.
fn bucket_sort_key(snapshot: &BucketSnapshot) -> (u8, usize, u8) {
    match snapshot.bucket_kind.as_str() {
        "tlb" => (0, 0, 0),
        "cache" => (1, snapshot.level_index.unwrap_or(0), if snapshot.side.as_deref() == Some("instruction") { 0 } else { 1 }),
        "ram" => (2, 0, 0),
        _ => (3, 0, 0),
    }
}

fn bucket_snapshot(bucket: Bucket, stats: &BucketStats, instructions_completed: u64) -> BucketSnapshot {
    let (bucket_kind, level_index, side) = match bucket {
        Bucket::Tlb => ("tlb", None, None),
        Bucket::Cache { level_index, side } => {
            let side_str = match side {
                Side::Unified => "unified",
                Side::Instruction => "instruction",
                Side::Data => "data",
            };
            ("cache", Some(level_index), Some(side_str))
        }
        Bucket::Ram => ("ram", None, None),
        Bucket::Disk => ("disk", None, None),
    };
    BucketSnapshot {
        bucket_kind: bucket_kind.to_string(),
        level_index,
        side: side.map(|s| s.to_string()),
        accesses: stats.accesses,
        hits: stats.hits,
        misses: stats.misses,
        cycles: stats.cycles,
        mpki: if instructions_completed > 0 { stats.misses as f64 / instructions_completed as f64 * 1000.0 } else { 0.0 },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketSnapshot {
    pub bucket_kind: String,
    pub level_index: Option<usize>,
    pub side: Option<String>,
    pub accesses: u64,
    pub hits: u64,
    pub misses: u64,
    pub cycles: u64,
    pub mpki: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSnapshot {
    pub instructions_completed: u64,
    pub total_memory_accesses: u64,
    pub top_of_chain_instruction_accesses: u64,
    pub top_of_chain_data_accesses: u64,
    pub total_cycles: u64,
    pub amat: f64,
    pub cpi: f64,
    pub buckets: Vec<BucketSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_accumulator_snapshots_all_zero() {
        let acc = StatsAccumulator::new();
        let snap = acc.snapshot();
        assert_eq!(snap.instructions_completed, 0);
        assert_eq!(snap.total_memory_accesses, 0);
        assert_eq!(snap.total_cycles, 0);
        assert_eq!(snap.amat, 0.0);
        assert_eq!(snap.cpi, 0.0);
        assert!(snap.buckets.is_empty());
    }

    #[test]
    fn record_access_grows_cycles_and_the_matching_bucket() {
        let mut acc = StatsAccumulator::new();
        acc.record_access(Bucket::Cache { level_index: 0, side: Side::Unified }, AccessKind::Load, 3, true);
        let snap = acc.snapshot();
        assert_eq!(snap.total_cycles, 3);
        assert_eq!(snap.top_of_chain_data_accesses, 1);
        assert_eq!(snap.top_of_chain_instruction_accesses, 0);
        let bucket = snap.buckets.iter().find(|b| b.bucket_kind == "cache").unwrap();
        assert_eq!(bucket.accesses, 1);
        assert_eq!(bucket.cycles, 3);
    }

    #[test]
    fn record_access_for_a_fetch_grows_the_instruction_side_denominator() {
        let mut acc = StatsAccumulator::new();
        acc.record_access(Bucket::Tlb, AccessKind::Fetch, 1, true);
        let snap = acc.snapshot();
        assert_eq!(snap.top_of_chain_instruction_accesses, 1);
        assert_eq!(snap.top_of_chain_data_accesses, 0);
    }

    #[test]
    fn record_access_without_top_of_chain_does_not_grow_either_denominator() {
        let mut acc = StatsAccumulator::new();
        acc.record_access(Bucket::Cache { level_index: 1, side: Side::Data }, AccessKind::Load, 5, false);
        let snap = acc.snapshot();
        assert_eq!(snap.total_memory_accesses, 0);
        assert_eq!(snap.total_cycles, 5, "cycles still count even when this access is not the top of the chain");
    }

    #[test]
    fn record_outcome_tallies_hits_and_misses_without_touching_cycles_or_accesses() {
        let mut acc = StatsAccumulator::new();
        acc.record_outcome(Bucket::Ram, LevelOutcome::Hit);
        acc.record_outcome(Bucket::Ram, LevelOutcome::Miss);
        acc.record_outcome(Bucket::Ram, LevelOutcome::Hit);
        let snap = acc.snapshot();
        let ram = snap.buckets.iter().find(|b| b.bucket_kind == "ram").unwrap();
        assert_eq!(ram.hits, 2);
        assert_eq!(ram.misses, 1);
        assert_eq!(ram.accesses, 0, "record_outcome alone must not create an access");
        assert_eq!(snap.total_cycles, 0);
    }

    #[test]
    fn a_harvard_levels_two_sides_land_in_two_separate_buckets() {
        let mut acc = StatsAccumulator::new();
        acc.record_access(Bucket::Cache { level_index: 0, side: Side::Instruction }, AccessKind::Fetch, 1, true);
        acc.record_access(Bucket::Cache { level_index: 0, side: Side::Data }, AccessKind::Load, 1, false);
        let snap = acc.snapshot();
        let cache_buckets: Vec<_> = snap.buckets.iter().filter(|b| b.bucket_kind == "cache").collect();
        assert_eq!(cache_buckets.len(), 2);
        assert!(cache_buckets.iter().any(|b| b.side.as_deref() == Some("instruction")));
        assert!(cache_buckets.iter().any(|b| b.side.as_deref() == Some("data")));
    }

    #[test]
    fn record_instruction_complete_grows_instructions_and_one_execute_cycle() {
        let mut acc = StatsAccumulator::new();
        acc.record_instruction_complete();
        acc.record_instruction_complete();
        let snap = acc.snapshot();
        assert_eq!(snap.instructions_completed, 2);
        assert_eq!(snap.total_cycles, 2);
    }

    #[test]
    fn amat_excludes_each_instructions_own_execute_cycle() {
        let mut acc = StatsAccumulator::new();
        acc.record_access(Bucket::Cache { level_index: 0, side: Side::Unified }, AccessKind::Load, 10, true);
        acc.record_instruction_complete();
        let snap = acc.snapshot();
        // 10 memory cycles + 1 execute cycle = 11 total, but AMAT only counts the 10 memory cycles over 1 access.
        assert_eq!(snap.total_cycles, 11);
        assert_eq!(snap.amat, 10.0);
    }

    #[test]
    fn cpi_divides_total_cycles_by_instructions_completed() {
        let mut acc = StatsAccumulator::new();
        acc.record_access(Bucket::Ram, AccessKind::Load, 8, true);
        acc.record_instruction_complete();
        acc.record_instruction_complete();
        let snap = acc.snapshot();
        // 8 memory cycles + 2 execute cycles = 10 total, over 2 instructions.
        assert_eq!(snap.cpi, 5.0);
    }

    #[test]
    fn mpki_divides_total_misses_by_instructions_completed_times_a_thousand() {
        let mut acc = StatsAccumulator::new();
        acc.record_outcome(Bucket::Tlb, LevelOutcome::Miss);
        acc.record_outcome(Bucket::Ram, LevelOutcome::Miss);
        acc.record_outcome(Bucket::Ram, LevelOutcome::Hit);
        acc.record_instruction_complete();
        acc.record_instruction_complete();
        let snap = acc.snapshot();
        let ram = snap.buckets.iter().find(|b| b.bucket_kind == "ram").unwrap();
        assert_eq!(ram.mpki, 500.0);
    }

    #[test]
    fn mpki_is_zero_with_no_instructions_completed() {
        let mut acc = StatsAccumulator::new();
        acc.record_outcome(Bucket::Tlb, LevelOutcome::Miss);
        let snap = acc.snapshot();
        assert_eq!(snap.buckets[0].mpki, 0.0);
    }
}
