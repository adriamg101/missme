use super::backing_store::DataStore;
use super::cache::SetAssociativeCache;
use super::config::{CacheConfig, CacheEntry, HierarchyConfig, Level, WritePolicy};
use super::seeded_random;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    Fetch,
    Load,
    Store,
}

#[derive(Debug, Clone)]
pub struct MemoryAccess {
    pub kind: AccessKind,
    pub address: u64,
    pub size: u8,
    pub store_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelId {
    Tlb,
    Cache(usize),
    Ram,
    Disk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelOutcome {
    Hit,
    Miss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelResult {
    pub level: LevelId,
    pub outcome: LevelOutcome,
    pub address: u64,
    pub access_kind: AccessKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessResult {
    pub load_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheSide {
    Unified,
    Instruction,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheLevelInfo {
    pub associativity: usize,
    pub block_size: u32,
    pub num_sets: u64,
    pub words_per_line: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheWaySnapshot {
    pub valid: bool,
    pub dirty: bool,
    pub tag: u64,
    pub recency_rank: Option<usize>,
    pub words: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSetSnapshot {
    pub set_index: u64,
    pub ways: Vec<CacheWaySnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RamBlockSnapshot {
    pub address: u64,
    pub words: Vec<u32>,
    pub populated: bool,
    pub dirty: bool,
}

fn entry_slot_for_side(side: CacheSide) -> EntrySlot {
    match side {
        CacheSide::Unified => EntrySlot::Unified,
        CacheSide::Instruction => EntrySlot::I,
        CacheSide::Data => EntrySlot::D,
    }
}

struct CacheLevelState {
    unified: Option<(CacheConfig, u32, SetAssociativeCache)>,
    i: Option<(CacheConfig, u32, SetAssociativeCache)>,
    d: Option<(CacheConfig, u32, SetAssociativeCache)>,
}

/// `side_tag` (0/1/2 for unified/instruction/data) makes sure two sides of
/// the same level don't end up with the same pre-loaded dirty pattern. For
/// example a level with separate instruction and data caches gets a
/// different starting pattern for each side, even though they share the
/// same level number, settings, and overall session seed.
fn cache_state_for(level: &Level, level_index: usize, session_seed: u64) -> CacheLevelState {
    let build = |entry: &CacheEntry, side_tag: u64| {
        let seed = seeded_random::value(session_seed, &[5, level_index as u64, side_tag]);
        let cache = SetAssociativeCache::with_dirty_seed(entry.config.associativity.max(1) as usize, entry.config.dirty_percent, seed);
        (entry.config, entry.exp, cache)
    };
    CacheLevelState {
        unified: level.unified.as_ref().map(|e| build(e, 0)),
        i: level.i.as_ref().map(|e| build(e, 1)),
        d: level.d.as_ref().map(|e| build(e, 2)),
    }
}

/// Says which cache entry (unified, instruction, or data) a given access
/// belongs to, so we can look that up and then use the entry separately
/// afterward.
enum EntrySlot {
    Unified,
    I,
    D,
}

impl CacheLevelState {
    fn slot_for(&self, is_fetch: bool) -> Option<EntrySlot> {
        if self.unified.is_some() {
            return Some(EntrySlot::Unified);
        }
        if is_fetch {
            self.i.is_some().then_some(EntrySlot::I)
        } else {
            self.d.is_some().then_some(EntrySlot::D)
        }
    }

    fn entry_mut(&mut self, slot: &EntrySlot) -> &mut (CacheConfig, u32, SetAssociativeCache) {
        match slot {
            EntrySlot::Unified => self.unified.as_mut().expect("slot_for chose Unified because it is Some"),
            EntrySlot::I => self.i.as_mut().expect("slot_for chose I because it is Some"),
            EntrySlot::D => self.d.as_mut().expect("slot_for chose D because it is Some"),
        }
    }

    fn entry(&self, slot: &EntrySlot) -> Option<&(CacheConfig, u32, SetAssociativeCache)> {
        match slot {
            EntrySlot::Unified => self.unified.as_ref(),
            EntrySlot::I => self.i.as_ref(),
            EntrySlot::D => self.d.as_ref(),
        }
    }
}

enum Cursor {
    Tlb,
    Cache(usize),
    Ram,
}

pub struct MemoryHierarchySimulator {
    config: HierarchyConfig,
    tlb: SetAssociativeCache,
    levels: Vec<CacheLevelState>,
    ram: DataStore,
    #[allow(dead_code)] // set up for future disk-level modeling in TT4.3; not used yet in this branch, see design notes
    disk: DataStore,
    in_progress: Option<MemoryAccess>,
    cursor: Option<Cursor>,
    finished_result: Option<AccessResult>,
}

impl MemoryHierarchySimulator {
    /// `seed` is the session's seed, already decided by the caller (either
    /// what the user chose, or a randomly generated one if they left it
    /// blank, see `seeded_random::generate_seed`). It's resolved before this
    /// function runs so the same seed can also be used for the `ram` data
    /// store passed in below.
    pub fn new(config: HierarchyConfig, ram: DataStore, disk: DataStore, seed: u64) -> Self {
        let tlb = SetAssociativeCache::new(config.tlb.associativity.max(1) as usize);
        let levels = config.hierarchy.iter().enumerate().map(|(i, level)| cache_state_for(level, i, seed)).collect();
        Self {
            config,
            tlb,
            levels,
            ram,
            disk,
            in_progress: None,
            cursor: None,
            finished_result: None,
        }
    }

    pub fn is_access_finished(&self) -> bool {
        self.finished_result.is_some()
    }

    pub fn preload_ram(&mut self, base_address: u64, data: &[u8]) {
        self.ram.preload(base_address, data);
    }

    pub fn begin_access(&mut self, access: MemoryAccess) -> LevelResult {
        assert!(self.in_progress.is_none(), "an access is already in progress");
        self.in_progress = Some(access);
        self.finished_result = None;
        let first_cursor = if self.config.vm_enabled { Cursor::Tlb } else { Cursor::Cache(0) };
        self.cursor = Some(first_cursor);
        self.advance()
    }

    pub fn step(&mut self) -> LevelResult {
        assert!(self.cursor.is_some(), "no access is in progress");
        assert!(!self.is_access_finished(), "the in-progress access already finished");
        self.advance()
    }

    pub fn take_result(&mut self) -> AccessResult {
        self.in_progress = None;
        self.cursor = None;
        self.finished_result.take().expect("the access hasn't finished yet")
    }

    pub fn cache_level_info(&self, level_index: usize, side: CacheSide) -> Option<CacheLevelInfo> {
        let level_state = self.levels.get(level_index)?;
        let slot = entry_slot_for_side(side);
        let (cache_config, exp, cache) = level_state.entry(&slot)?;
        let associativity = cache.associativity();
        let num_sets = ((1u64 << *exp) / (associativity as u64).max(1)).max(1);
        let words_per_line = (cache_config.block_size / 4).max(1);
        Some(CacheLevelInfo { associativity, block_size: cache_config.block_size, num_sets, words_per_line })
    }

    fn cache_set_ways(&self, level_index: usize, side: CacheSide, set_index: u64, info: &CacheLevelInfo) -> Vec<CacheWaySnapshot> {
        let level_state = &self.levels[level_index];
        let slot = entry_slot_for_side(side);
        let (_, _, cache) = level_state.entry(&slot).expect("caller already confirmed this side exists via cache_level_info");
        cache
            .snapshot_set(set_index)
            .into_iter()
            .map(|line| {
                // Only a line that a real access actually put there (it has
                // a recency rank) reads its words from RAM. A pre-loaded
                // dirty line that was never really accessed has a tag that's
                // just a random number, not a real address, so showing its
                // contents would reveal garbage the program never touched.
                // We report no words for it instead, and the frontend shows
                // a dash.
                let words = if line.valid && line.recency_rank.is_some() {
                    let base_address = line.tag * info.block_size as u64;
                    self.ram
                        .read_block(base_address, info.block_size as usize)
                        .chunks(4)
                        .map(|chunk| {
                            let mut bytes = [0u8; 4];
                            bytes[..chunk.len()].copy_from_slice(chunk);
                            u32::from_le_bytes(bytes)
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                CacheWaySnapshot { valid: line.valid, dirty: line.dirty, tag: line.tag, recency_rank: line.recency_rank, words }
            })
            .collect()
    }

    /// Returns only the requested range of sets (clamped to how many sets
    /// the level actually has), instead of every set at once. The number of
    /// sets can be huge for a big, low-associativity cache, so the frontend
    /// has to page through sets in chunks, the same way it pages through
    /// RAM blocks with `ram_blocks`, rather than asking for a whole level
    /// in one go.
    pub fn cache_sets(&self, level_index: usize, side: CacheSide, start_set: u64, count: u32) -> Option<Vec<CacheSetSnapshot>> {
        let info = self.cache_level_info(level_index, side)?;
        let end = start_set.saturating_add(count as u64).min(info.num_sets);
        Some(
            (start_set.min(info.num_sets)..end)
                .map(|set_index| CacheSetSnapshot { set_index, ways: self.cache_set_ways(level_index, side, set_index, &info) })
                .collect(),
        )
    }

    pub fn ram_blocks(&self, start_address: u64, count: u32) -> Vec<RamBlockSnapshot> {
        let block_size = self.ram.block_size() as u64;
        (0..count as u64)
            .map(|i| {
                let address = start_address + i * block_size;
                let words = self
                    .ram
                    .read_block(address, block_size as usize)
                    .chunks(4)
                    .map(|chunk| {
                        let mut bytes = [0u8; 4];
                        bytes[..chunk.len()].copy_from_slice(chunk);
                        u32::from_le_bytes(bytes)
                    })
                    .collect();
                RamBlockSnapshot { address, words, populated: self.ram.is_populated(address), dirty: self.ram.is_dirty(address) }
            })
            .collect()
    }

    fn advance(&mut self) -> LevelResult {
        match self.cursor.take().expect("advance called without a cursor") {
            Cursor::Tlb => self.step_tlb(),
            Cursor::Cache(index) => self.step_cache(index),
            Cursor::Ram => self.step_ram(),
        }
    }

    fn step_tlb(&mut self) -> LevelResult {
        let access = self.in_progress.as_ref().unwrap();
        let address = access.address;
        let access_kind = access.kind;
        let page = access.address / self.config.tlb.page_size as u64;
        let outcome = match self.tlb.lookup(0, page) {
            super::cache::LookupOutcome::Hit => LevelOutcome::Hit,
            super::cache::LookupOutcome::Miss => {
                self.tlb.insert(0, page, false);
                LevelOutcome::Miss
            }
        };
        self.cursor = Some(Cursor::Cache(0));
        LevelResult { level: LevelId::Tlb, outcome, address, access_kind }
    }

    /// Reads the actual bytes for a finished Load or Fetch straight from the
    /// backing data store. This simulator only tracks which blocks are
    /// cached, not the byte contents at each level, so RAM is always the
    /// real source of the data, whether it was a hit or a miss. This uses
    /// `access_byte` rather than the passive `read_byte`, because a real
    /// Load is exactly the moment an untouched pre-loaded cell should
    /// actually become real data. The inspection views never trigger that
    /// on their own.
    fn read_load_bytes(&mut self, address: u64, size: u8) -> Vec<u8> {
        (0..size).map(|i| self.ram.access_byte(address + i as u64)).collect()
    }

    fn finish_with_load(&mut self, access: &MemoryAccess) {
        let load_bytes = match access.kind {
            AccessKind::Load => Some(self.read_load_bytes(access.address, access.size)),
            AccessKind::Fetch | AccessKind::Store => None,
        };
        self.finished_result = Some(AccessResult { load_bytes });
    }

    fn write_store_bytes(&mut self, access: &MemoryAccess) {
        let bytes = access.store_bytes.clone().expect("a Store access always carries store_bytes");
        for (i, byte) in bytes.iter().enumerate() {
            self.ram.write_byte(access.address + i as u64, *byte);
        }
    }

    fn step_cache(&mut self, index: usize) -> LevelResult {
        if index >= self.levels.len() {
            self.cursor = Some(Cursor::Ram);
            return self.step_ram();
        }

        let access = self.in_progress.as_ref().unwrap().clone();
        let is_fetch = access.kind == AccessKind::Fetch;

        let Some(slot) = self.levels[index].slot_for(is_fetch) else {
            // This level doesn't have a cache for this kind of access (for
            // example a split level with no instruction cache configured),
            // so skip it entirely and move straight to the next level.
            self.cursor = Some(Cursor::Cache(index + 1));
            return self.step_cache(index + 1);
        };

        let (cache_config, exp, cache) = self.levels[index].entry_mut(&slot);
        let block_size = cache_config.block_size.max(1) as u64;
        let write_policy = cache_config.write_policy;
        let write_allocate = cache_config.write_allocate;
        let num_sets = ((1u64 << *exp) / cache_config.associativity.max(1) as u64).max(1);
        let tag = access.address / block_size;
        let set_index = tag % num_sets;

        let outcome = match cache.lookup(set_index, tag) {
            super::cache::LookupOutcome::Hit => {
                match access.kind {
                    AccessKind::Store if write_policy == WritePolicy::WriteBack => {
                        // Write-back hit: mark the line dirty and stop here.
                        // Since this model doesn't keep separate byte
                        // contents per level, the write still lands in the
                        // data store right away, because the walk down to
                        // RAM will never happen for this access.
                        cache.mark_dirty(set_index, tag);
                        self.write_store_bytes(&access);
                        self.finished_result = Some(AccessResult { load_bytes: None });
                    }
                    AccessKind::Store => {
                        // Write-through hit: keep going down through the
                        // remaining levels to RAM, where the write actually
                        // happens.
                        self.cursor = Some(Cursor::Cache(index + 1));
                    }
                    AccessKind::Load | AccessKind::Fetch => {
                        self.finish_with_load(&access);
                    }
                }
                LevelOutcome::Hit
            }
            super::cache::LookupOutcome::Miss => {
                if access.kind != AccessKind::Store || write_allocate {
                    let dirty = access.kind == AccessKind::Store && write_policy == WritePolicy::WriteBack;
                    cache.insert(set_index, tag, dirty);
                }
                self.cursor = Some(Cursor::Cache(index + 1));
                LevelOutcome::Miss
            }
        };

        LevelResult { level: LevelId::Cache(index), outcome, address: access.address, access_kind: access.kind }
    }

    fn step_ram(&mut self) -> LevelResult {
        let access = self.in_progress.as_ref().unwrap().clone();
        match access.kind {
            AccessKind::Store => {
                self.write_store_bytes(&access);
                self.finished_result = Some(AccessResult { load_bytes: None });
            }
            AccessKind::Load | AccessKind::Fetch => {
                self.finish_with_load(&access);
            }
        }
        LevelResult { level: LevelId::Ram, outcome: LevelOutcome::Hit, address: access.address, access_kind: access.kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::{DiskConfig, InclusionPolicy, RamConfig, ReplacementPolicy, TlbConfig, TlbReplacementPolicy};
    use crate::models::interpreter::riscv::Base;

    fn cache_config() -> CacheConfig {
        CacheConfig {
            block_size: 4,
            associativity: 1,
            replacement: ReplacementPolicy::Lru,
            write_policy: WritePolicy::WriteBack,
            write_allocate: true,
            hit_time: 1,
            dirty_percent: 0,
        }
    }

    fn write_through_cache_config() -> CacheConfig {
        CacheConfig { write_policy: WritePolicy::WriteThrough, ..cache_config() }
    }

    fn one_level_config_with(vm_enabled: bool, cache_cfg: CacheConfig) -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled,
            hierarchy: vec![Level {
                id: "l1".into(),
                harvard: false,
                unified: Some(CacheEntry { exp: 1, config: cache_cfg }), // 2 lines, 1 way each, so 2 sets
                i: None,
                d: None,
                inclusion: InclusionPolicy::Nine,
                bus_width: 32,
            }],
            tlb: TlbConfig {
                page_size: 4096,
                page_table_levels: 1,
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    fn one_level_config(vm_enabled: bool) -> HierarchyConfig {
        one_level_config_with(vm_enabled, cache_config())
    }

    fn load(address: u64) -> MemoryAccess {
        MemoryAccess { kind: AccessKind::Load, address, size: 4, store_bytes: None }
    }

    fn store(address: u64, bytes: Vec<u8>) -> MemoryAccess {
        MemoryAccess { kind: AccessKind::Store, address, size: bytes.len() as u8, store_bytes: Some(bytes) }
    }

    fn fetch(address: u64) -> MemoryAccess {
        MemoryAccess { kind: AccessKind::Fetch, address, size: 4, store_bytes: None }
    }

    fn sim(config: HierarchyConfig) -> MemoryHierarchySimulator {
        let ram = DataStore::with_dirty_percent_seeded(4, config.ram.dirty_percent, 0);
        MemoryHierarchySimulator::new(config, ram, DataStore::new(4), 0)
    }

    #[test]
    fn first_touch_of_an_address_misses_at_l1_then_hits_ram() {
        let mut mem = sim(one_level_config(false));
        let first = mem.begin_access(load(0x10000000));
        assert_eq!(first.level, LevelId::Cache(0));
        assert_eq!(first.outcome, LevelOutcome::Miss);

        let second = mem.step();
        assert_eq!(second.level, LevelId::Ram);
        assert_eq!(second.outcome, LevelOutcome::Hit);
        assert!(mem.is_access_finished());
    }

    #[test]
    fn second_touch_of_the_same_address_hits_at_l1() {
        let mut mem = sim(one_level_config(false));
        mem.begin_access(load(0x10000000));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let result = mem.begin_access(load(0x10000000));
        assert_eq!(result.level, LevelId::Cache(0));
        assert_eq!(result.outcome, LevelOutcome::Hit);
        assert!(mem.is_access_finished());
    }

    #[test]
    fn load_result_reflects_bytes_previously_stored() {
        let mut mem = sim(one_level_config(false));
        mem.begin_access(store(0x10000000, vec![0xAA, 0xBB, 0xCC, 0xDD]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        mem.begin_access(load(0x10000000));
        while !mem.is_access_finished() {
            mem.step();
        }
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![0xAA, 0xBB, 0xCC, 0xDD]));
    }

    #[test]
    fn write_back_store_hit_terminates_the_access_immediately() {
        let mut mem = sim(one_level_config(false));

        // Prime the line: the first store misses, gets added to the cache,
        // and since there's no lower cache level, ends up written to RAM.
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // A second store to the same address now hits at L1. A write-back
        // hit must finish the access right there, with just one result, no
        // need to keep going down to RAM.
        let result = mem.begin_access(store(0x10000000, vec![0xAA, 0xBB, 0xCC, 0xDD]));
        assert_eq!(result.level, LevelId::Cache(0));
        assert_eq!(result.outcome, LevelOutcome::Hit);
        assert!(mem.is_access_finished());
        mem.take_result();

        // The write-back hit must still have applied the write straight to
        // RAM (this simulator has no separate byte contents per level, so
        // RAM is where a write-back hit's data actually ends up).
        let load_result = mem.begin_access(load(0x10000000));
        assert_eq!(load_result.level, LevelId::Cache(0));
        assert_eq!(load_result.outcome, LevelOutcome::Hit);
        assert!(mem.is_access_finished());
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![0xAA, 0xBB, 0xCC, 0xDD]));
    }

    #[test]
    fn write_through_store_hit_does_not_terminate_and_reaches_ram() {
        let mut mem = sim(one_level_config_with(false, write_through_cache_config()));

        // Prime the line: the first store misses, gets added to the cache
        // (not dirty, since write-through never leaves a line dirty), and
        // writes to RAM.
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // A second store to the same address hits at L1, but a write-through
        // hit must NOT finish the access there: it keeps going down to RAM,
        // where write-through always applies the write.
        let first = mem.begin_access(store(0x10000000, vec![0xAA, 0xBB, 0xCC, 0xDD]));
        assert_eq!(first.level, LevelId::Cache(0));
        assert_eq!(first.outcome, LevelOutcome::Hit);
        assert!(!mem.is_access_finished());

        let second = mem.step();
        assert_eq!(second.level, LevelId::Ram);
        assert_eq!(second.outcome, LevelOutcome::Hit);
        assert!(mem.is_access_finished());
        mem.take_result();

        mem.begin_access(load(0x10000000));
        while !mem.is_access_finished() {
            mem.step();
        }
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![0xAA, 0xBB, 0xCC, 0xDD]));
    }

    #[test]
    fn vm_enabled_checks_the_tlb_before_the_cache() {
        let mut mem = sim(one_level_config(true));
        let first = mem.begin_access(load(0x10000000));
        assert_eq!(first.level, LevelId::Tlb);
        assert_eq!(first.outcome, LevelOutcome::Miss);

        let second = mem.step();
        assert_eq!(second.level, LevelId::Cache(0));
        assert_eq!(second.outcome, LevelOutcome::Miss);
    }

    #[test]
    fn vm_disabled_never_produces_a_tlb_level_result() {
        let mut mem = sim(one_level_config(false));
        let first = mem.begin_access(load(0x10000000));
        assert_ne!(first.level, LevelId::Tlb);
    }

    #[test]
    fn fetch_walks_the_hierarchy_with_the_same_shape_as_an_equivalent_load() {
        // A fresh Fetch to an address must miss at L1 then hit at RAM, just
        // like a fresh Load to the same address would. The hierarchy
        // doesn't treat instruction fetches specially, beyond labeling the
        // result with the right access kind.
        let mut fetch_mem = sim(one_level_config(false));
        let fetch_first = fetch_mem.begin_access(fetch(0x10000000));
        assert_eq!(fetch_first.level, LevelId::Cache(0));
        assert_eq!(fetch_first.outcome, LevelOutcome::Miss);
        assert_eq!(fetch_first.access_kind, AccessKind::Fetch);
        let fetch_second = fetch_mem.step();
        assert_eq!(fetch_second.level, LevelId::Ram);
        assert_eq!(fetch_second.outcome, LevelOutcome::Hit);
        assert!(fetch_mem.is_access_finished());

        let mut load_mem = sim(one_level_config(false));
        let load_first = load_mem.begin_access(load(0x10000000));
        assert_eq!(load_first.level, fetch_first.level);
        assert_eq!(load_first.outcome, fetch_first.outcome);
        let load_second = load_mem.step();
        assert_eq!(load_second.level, fetch_second.level);
        assert_eq!(load_second.outcome, fetch_second.outcome);
    }

    fn harvard_d_only_then_unified_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            hierarchy: vec![
                Level {
                    id: "l1".into(),
                    harvard: true,
                    unified: None,
                    i: None,
                    d: Some(CacheEntry { exp: 1, config: cache_config() }),
                    inclusion: InclusionPolicy::Nine,
                    bus_width: 32,
                },
                Level {
                    id: "l2".into(),
                    harvard: false,
                    unified: Some(CacheEntry { exp: 1, config: cache_config() }),
                    i: None,
                    d: None,
                    inclusion: InclusionPolicy::Nine,
                    bus_width: 32,
                },
            ],
            tlb: TlbConfig {
                page_size: 4096,
                page_table_levels: 1,
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    #[test]
    fn fetch_skips_a_harvard_level_with_no_instruction_side_configured() {
        // L1 is split with only a data cache configured, no instruction
        // cache. A Fetch access must skip straight past it to L2 without
        // producing a result of its own for L1, the same way a Store skips
        // a level that has no matching cache for it.
        let mut mem = sim(harvard_d_only_then_unified_hierarchy_config());
        let first = mem.begin_access(fetch(0x10000000));
        assert_eq!(first.level, LevelId::Cache(1), "L1 (d-only) must be skipped entirely for a fetch");
        assert_eq!(first.outcome, LevelOutcome::Miss);
    }

    #[test]
    #[should_panic]
    fn step_before_begin_access_panics() {
        let mut mem = sim(one_level_config(false));
        mem.step();
    }

    #[test]
    #[should_panic]
    fn begin_access_while_one_is_in_progress_panics() {
        let mut mem = sim(one_level_config(false));
        mem.begin_access(load(0x10000000));
        mem.begin_access(load(0x10000004));
    }

    #[test]
    #[should_panic]
    fn take_result_before_finished_panics() {
        let mut mem = sim(one_level_config(false));
        mem.begin_access(load(0x10000000));
        mem.take_result();
    }

    #[test]
    fn cache_level_info_reports_derived_sizing() {
        let mem = sim(one_level_config(false)); // 1 level, exp 1, associativity 1, block size 4, so 2 sets
        let info = mem.cache_level_info(0, CacheSide::Unified).unwrap();
        assert_eq!(info.associativity, 1);
        assert_eq!(info.block_size, 4);
        assert_eq!(info.num_sets, 2);
        assert_eq!(info.words_per_line, 1); // 4 bytes makes one 4-byte word
    }

    #[test]
    fn cache_level_info_is_none_for_a_side_the_level_does_not_have() {
        let mem = sim(one_level_config(false)); // unified only
        assert!(mem.cache_level_info(0, CacheSide::Instruction).is_none());
        assert!(mem.cache_level_info(5, CacheSide::Unified).is_none()); // level number too high
    }

    #[test]
    fn cache_sets_reflects_a_stored_line_and_its_words() {
        let mut mem = sim(one_level_config(false));
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let info = mem.cache_level_info(0, CacheSide::Unified).unwrap();
        assert_eq!(info.associativity, 1);
        assert_eq!(info.num_sets, 2);

        let sets = mem.cache_sets(0, CacheSide::Unified, 0, info.num_sets as u32).unwrap();
        assert_eq!(sets.len(), 2);

        // address 0x10000000 with block size 4 gives tag 0x4000000, and set index tag % 2 = 0
        let set0 = sets.iter().find(|s| s.set_index == 0).unwrap();
        let way = set0.ways.iter().find(|w| w.valid).expect("the stored line must be present");
        assert_eq!(way.tag, 0x10000000 / 4);
        assert_eq!(way.words, vec![u32::from_le_bytes([0x11, 0x22, 0x33, 0x44])]);
    }

    #[test]
    fn cache_sets_does_not_overflow_reconstructing_words_for_a_dirty_seeded_phantom_line() {
        // A pre-loaded dirty line's tag is just a random large number, not a
        // real address divided by the block size. Rebuilding its words used
        // to multiply that huge tag by the block size directly, which
        // overflowed and crashed the whole app the moment a cache view with
        // any dirty percent was opened on an untouched set.
        let mut cfg = cache_config();
        cfg.dirty_percent = 100;
        let mem = sim(one_level_config_with(false, cfg));

        let info = mem.cache_level_info(0, CacheSide::Unified).unwrap();
        let sets = mem.cache_sets(0, CacheSide::Unified, 0, info.num_sets as u32).unwrap();

        assert!(sets.iter().any(|s| s.ways.iter().any(|w| w.valid && w.dirty)));
    }

    #[test]
    fn cache_level_info_is_none_out_of_range_too() {
        let mem = sim(one_level_config(false));
        assert!(mem.cache_level_info(9, CacheSide::Unified).is_none());
        assert!(mem.cache_sets(9, CacheSide::Unified, 0, 10).is_none());
    }

    #[test]
    fn cache_sets_pages_a_window_instead_of_the_whole_level() {
        let mut mem = sim(one_level_config(false));
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let sets = mem.cache_sets(0, CacheSide::Unified, 1, 1).unwrap();
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].set_index, 1);
    }

    #[test]
    fn ram_blocks_does_not_reveal_a_dirty_seeded_block_that_was_never_really_accessed() {
        let mut cfg = one_level_config(false);
        cfg.ram.dirty_percent = 100;
        let mem = sim(cfg);

        let blocks = mem.ram_blocks(0x10000000, 1);
        assert!(!blocks[0].populated);
        assert!(blocks[0].dirty);
        assert_eq!(blocks[0].words, vec![0]); // no garbage leaked through a plain look
    }

    #[test]
    fn a_real_load_reveals_a_dirty_seeded_block_from_then_on() {
        let mut cfg = one_level_config(false);
        cfg.ram.dirty_percent = 100;
        let mut mem = sim(cfg);

        mem.begin_access(load(0x10000000));
        while !mem.is_access_finished() {
            mem.step();
        }
        let loaded = mem.take_result().load_bytes.unwrap();

        let blocks = mem.ram_blocks(0x10000000, 1);
        assert!(blocks[0].populated);
        assert!(!blocks[0].dirty);
        // What the program actually read matches what the display now shows.
        assert_eq!(blocks[0].words[0].to_le_bytes()[..4], loaded[..4]);
    }

    #[test]
    fn ram_blocks_reports_populated_and_empty_blocks() {
        let mut mem = sim(one_level_config(false));
        mem.begin_access(store(0x10000000, vec![0xDE, 0xAD, 0xBE, 0xEF]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let blocks = mem.ram_blocks(0x10000000, 3);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].address, 0x10000000);
        assert!(blocks[0].populated);
        assert_eq!(blocks[0].words, vec![u32::from_le_bytes([0xDE, 0xAD, 0xBE, 0xEF])]);
        assert!(!blocks[1].populated);
        assert_eq!(blocks[1].words, vec![0]);
    }

    #[test]
    fn ram_blocks_returns_zero_length_for_zero_count() {
        let mem = sim(one_level_config(false));
        assert_eq!(mem.ram_blocks(0x10000000, 0).len(), 0);
    }

    #[test]
    fn ram_blocks_word_matches_the_value_a_little_endian_sw_would_store() {
        // A real store of the 32-bit value 0x12345678 lays out its bytes
        // with the smallest byte first. The snapshot's words should
        // rebuild the exact same value the CPU stored, not one with the
        // bytes swapped around.
        let mut mem = sim(one_level_config(false));
        mem.begin_access(store(0x10000000, vec![0x78, 0x56, 0x34, 0x12]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let blocks = mem.ram_blocks(0x10000000, 1);
        assert_eq!(blocks[0].words, vec![0x12345678]);
    }
}
