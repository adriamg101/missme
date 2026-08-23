use super::backing_store::DataStore;
use super::cache::SetAssociativeCache;
use super::config::{CacheConfig, CacheEntry, HierarchyConfig, Level, WritePolicy};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessResult {
    pub load_bytes: Option<Vec<u8>>,
}

struct CacheLevelState {
    unified: Option<(CacheConfig, u32, SetAssociativeCache)>,
    i: Option<(CacheConfig, u32, SetAssociativeCache)>,
    d: Option<(CacheConfig, u32, SetAssociativeCache)>,
}

fn cache_state_for(level: &Level) -> CacheLevelState {
    let build = |entry: &CacheEntry| {
        (entry.config, entry.exp, SetAssociativeCache::new(entry.config.associativity.max(1) as usize))
    };
    CacheLevelState {
        unified: level.unified.as_ref().map(build),
        i: level.i.as_ref().map(build),
        d: level.d.as_ref().map(build),
    }
}

/// Which cache entry (of a level's `unified`/`i`/`d`) a given access kind
/// resolves to, so the borrow of `self.levels[index]` needed to answer that
/// question can end before the entry itself is borrowed again.
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
    #[allow(dead_code)] // wired for TT4.3's future disk-level modeling; unreachable this branch, see design notes
    disk: DataStore,
    in_progress: Option<MemoryAccess>,
    cursor: Option<Cursor>,
    finished_result: Option<AccessResult>,
}

impl MemoryHierarchySimulator {
    pub fn new(config: HierarchyConfig, ram: DataStore, disk: DataStore) -> Self {
        let tlb = SetAssociativeCache::new(config.tlb.associativity.max(1) as usize);
        let levels = config.hierarchy.iter().map(cache_state_for).collect();
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

    fn advance(&mut self) -> LevelResult {
        match self.cursor.take().expect("advance called without a cursor") {
            Cursor::Tlb => self.step_tlb(),
            Cursor::Cache(index) => self.step_cache(index),
            Cursor::Ram => self.step_ram(),
        }
    }

    fn step_tlb(&mut self) -> LevelResult {
        let access = self.in_progress.as_ref().unwrap();
        let page = access.address / self.config.tlb.page_size as u64;
        let outcome = match self.tlb.lookup(0, page) {
            super::cache::LookupOutcome::Hit => LevelOutcome::Hit,
            super::cache::LookupOutcome::Miss => {
                self.tlb.insert(0, page, false);
                LevelOutcome::Miss
            }
        };
        self.cursor = Some(Cursor::Cache(0));
        LevelResult { level: LevelId::Tlb, outcome }
    }

    /// Reads the ground-truth bytes for a finished `Load`/`Fetch` straight
    /// from the backing `DataStore` — this engine only tracks which blocks
    /// are cached, never per-level byte contents, so RAM is always the
    /// source of truth for "what the bytes actually are", cache hit or not.
    fn read_load_bytes(&self, address: u64, size: u8) -> Vec<u8> {
        (0..size).map(|i| self.ram.read_byte(address + i as u64)).collect()
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
            // This level has no entry for this access's kind (e.g. a harvard
            // level whose `i` was never configured) — skip it, it produces
            // no LevelResult, move straight to the next level.
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
                        // Write-back hit: mark the line dirty and stop the
                        // walk here. Per this branch's storage model (no
                        // per-level byte contents), the write still lands in
                        // the DataStore immediately since the walk will never
                        // reach RAM for this access.
                        cache.mark_dirty(set_index, tag);
                        self.write_store_bytes(&access);
                        self.finished_result = Some(AccessResult { load_bytes: None });
                    }
                    AccessKind::Store => {
                        // Write-through hit: propagate down through the
                        // remaining levels to RAM, where the write is
                        // actually applied.
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

        LevelResult { level: LevelId::Cache(index), outcome }
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
        LevelResult { level: LevelId::Ram, outcome: LevelOutcome::Hit }
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
                unified: Some(CacheEntry { exp: 1, config: cache_cfg }), // 2 lines / 1 way = 2 sets
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

    fn sim(config: HierarchyConfig) -> MemoryHierarchySimulator {
        MemoryHierarchySimulator::new(config, DataStore::new(4), DataStore::new(4))
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

        // Prime the line: first store misses, allocates, and (since there's
        // no lower cache level) eventually writes through to RAM.
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // Second store to the same address now hits at L1. A write-back hit
        // must terminate the access right there: one LevelResult, no need
        // to step further to reach RAM.
        let result = mem.begin_access(store(0x10000000, vec![0xAA, 0xBB, 0xCC, 0xDD]));
        assert_eq!(result.level, LevelId::Cache(0));
        assert_eq!(result.outcome, LevelOutcome::Hit);
        assert!(mem.is_access_finished());
        mem.take_result();

        // The write-back hit must still have applied the write to the
        // backing store immediately (this engine has no per-level byte
        // contents, so RAM is where a write-back hit's data actually lands).
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

        // Prime the line: first store misses, allocates (not dirty, since
        // write-through never leaves a line dirty), and writes to RAM.
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // Second store to the same address hits at L1, but a write-through
        // hit must NOT terminate the access: it keeps walking down to RAM,
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
}
