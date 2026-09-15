use super::backing_store::DataStore;
use super::cache::SetAssociativeCache;
use super::config::{CacheConfig, CacheEntry, HierarchyConfig, Level, WritePolicy};
use super::frames::FrameTable;
use super::page_table::PageTable;
use super::seeded_random;
use super::tlb::Tlb;
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
    pub page_translation_outcome: Option<LevelOutcome>,
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
    pub is_phantom: bool,
    pub order_value: Option<u64>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TlbInfo {
    pub associativity: usize,
    pub num_sets: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TlbSetSnapshot {
    pub set_index: u64,
    pub entries: Vec<super::tlb::TlbEntrySnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameContentSnapshot {
    pub frame: u64,
    pub vpn: Option<u64>,
    pub valid: bool,
    pub dirty: bool,
    pub words: Vec<u32>,
    /// One flag per word telling whether it was ever really written.
    /// A false one just defaults to zero and should be shown as empty, not as real data.
    pub populated: Vec<bool>,
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

/// The side tag stops two sides of the same level from starting with the same already loaded dirty pattern.
/// A level with separate instruction and data caches gets a different starting pattern for each side.
fn cache_state_for(level: &Level, level_index: usize, session_seed: u64) -> CacheLevelState {
    let build = |entry: &CacheEntry, side_tag: u64| {
        let seed = seeded_random::value(session_seed, &[5, level_index as u64, side_tag]);
        let cache = SetAssociativeCache::with_policy_and_dirty_seed(
            entry.config.associativity.max(1) as usize,
            entry.config.replacement.into(),
            entry.config.dirty_percent,
            seed,
        );
        (entry.config, entry.exp, cache)
    };
    CacheLevelState {
        unified: level.unified.as_ref().map(|e| build(e, 0)),
        i: level.i.as_ref().map(|e| build(e, 1)),
        d: level.d.as_ref().map(|e| build(e, 2)),
    }
}

/// Says which cache entry a given access belongs to, so it can be found and used again later.
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
    Walk,
    SwapOut,
    SwapIn,
    Cache(usize),
    Ram,
}

struct WalkState {
    vpn: u64,
    level: u8,
    chain_index: usize,
}

struct PendingFault {
    vpn: u64,
    frame: u64,
    evict: Option<super::frames::SwapOut>,
}

pub struct MemoryHierarchySimulator {
    config: HierarchyConfig,
    tlb: Tlb,
    page_table: PageTable,
    frames: FrameTable,
    levels: Vec<CacheLevelState>,
    ram: DataStore,
    disk: DataStore,
    in_progress: Option<MemoryAccess>,
    cursor: Option<Cursor>,
    finished_result: Option<AccessResult>,
    translated_address: Option<u64>,
    walk: Option<WalkState>,
    pending_fault: Option<PendingFault>,
}

impl MemoryHierarchySimulator {
    /// The seed is chosen by the caller before this runs, either the value the user picked or a fresh random one.
    /// The same seed is also used for the RAM store passed in below.
    pub fn new(config: HierarchyConfig, ram: DataStore, disk: DataStore, seed: u64) -> Self {
        let address_bits: u32 = match config.isa {
            crate::models::interpreter::riscv::Base::Rv32i => 32,
            crate::models::interpreter::riscv::Base::Rv64i => 64,
        };
        let tlb = Tlb::with_policy_and_seed(config.tlb.entries, config.tlb.associativity, config.tlb.replacement.into(), seed);
        // A level cannot claim more bits than the address has left after the page offset.
        let offset_bits = config.page_size.max(1).trailing_zeros();
        let total_vpn_bits = address_bits.saturating_sub(offset_bits);
        let pt_levels = config.ram.page_table_levels.min(total_vpn_bits.max(1) as u8).max(1);
        // The page table's own real addresses start right after the frame pool, so it never overlaps real data.
        let pt_base = config.ram.frames as u64 * config.page_size as u64;
        let page_table = PageTable::new(pt_levels, config.page_size, address_bits, pt_base);
        let frames = FrameTable::new(config.page_size, config.ram.frames.saturating_mul(config.page_size), config.ram.frame_allocation, seed);
        let levels = config.hierarchy.iter().enumerate().map(|(i, level)| cache_state_for(level, i, seed)).collect();
        Self {
            config,
            tlb,
            page_table,
            frames,
            levels,
            ram,
            disk,
            in_progress: None,
            cursor: None,
            finished_result: None,
            translated_address: None,
            walk: None,
            pending_fault: None,
        }
    }

    pub fn is_access_finished(&self) -> bool {
        self.finished_result.is_some()
    }

    pub fn config(&self) -> &HierarchyConfig {
        &self.config
    }

    /// Seeds the program's starting bytes into whichever store is really the backing memory for this config.
    /// Under VM a fresh page always comes from disk, so this preloads the disk instead of RAM.
    pub fn preload_ram(&mut self, base_address: u64, data: &[u8]) {
        if self.config.vm_enabled {
            self.disk.preload(base_address, data);
        } else {
            self.ram.preload(base_address, data);
        }
    }

    pub fn begin_access(&mut self, access: MemoryAccess) -> LevelResult {
        assert!(self.in_progress.is_none(), "an access is already in progress");
        self.in_progress = Some(access);
        self.finished_result = None;
        self.translated_address = None;
        self.walk = None;
        self.pending_fault = None;
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
                // Only a real, not a made up, line reads its words from RAM.
                // A made up line's tag is just a random number, so we report no words for it and the frontend shows a dash.
                let words = if line.valid && !line.is_phantom {
                    let base_address = line.tag * info.block_size as u64;
                    // Under VM nothing real ever reads or writes RAM, so the frame table is the real store to read from.
                    let raw_bytes = if self.config.vm_enabled {
                        self.frames.read_bytes(base_address, info.block_size as usize)
                    } else {
                        self.ram.read_block(base_address, info.block_size as usize)
                    };
                    raw_bytes
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
                CacheWaySnapshot { valid: line.valid, dirty: line.dirty, tag: line.tag, is_phantom: line.is_phantom, order_value: line.order_value, words }
            })
            .collect()
    }

    /// Returns only the requested range of sets, clamped to how many the level actually has.
    /// A big cache can have far too many sets to send at once, so the frontend pages through them in chunks.
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
            Cursor::Walk => self.step_walk(),
            Cursor::SwapOut => self.step_swap_out(),
            Cursor::SwapIn => self.step_swap_in(),
            Cursor::Cache(index) => self.step_cache(index),
            Cursor::Ram => self.step_ram(),
        }
    }

    fn effective_address(&self) -> u64 {
        self.translated_address.unwrap_or_else(|| self.in_progress.as_ref().unwrap().address)
    }

    /// Turns a resolved frame into the real physical address the rest of the pipeline uses.
    /// Must be called every time a page gets translated, or the access would wrongly use the virtual address.
    fn physical_address_for(&self, ppn: u64) -> u64 {
        let page_size = self.config.page_size as u64;
        let offset = self.in_progress.as_ref().unwrap().address % page_size;
        ppn * page_size + offset
    }

    /// Removes every cached line whose address falls inside this frame.
    /// Called the moment a frame gets handed to a different page.
    fn invalidate_physical_cache_lines_for_frame(&mut self, frame: u64) {
        let page_size = self.config.page_size as u64;
        let base = frame * page_size;
        let end = base + page_size;
        for level in &mut self.levels {
            for slot in [level.unified.as_mut(), level.i.as_mut(), level.d.as_mut()] {
                let Some((cache_config, exp, cache)) = slot else { continue };
                let block_size = cache_config.block_size.max(1) as u64;
                let num_sets = ((1u64 << *exp) / cache_config.associativity.max(1) as u64).max(1);
                let start_tag = base / block_size;
                let end_tag = (end - 1) / block_size;
                for tag in start_tag..=end_tag {
                    cache.invalidate(tag % num_sets, tag);
                }
            }
        }
    }

    fn resolve_page_walk_start_index(&self) -> usize {
        let start_id = &self.config.tlb.page_walk_start;
        if start_id == "RAM" {
            return self.levels.len();
        }
        self.config.hierarchy.iter().position(|l| &l.id == start_id).unwrap_or(self.levels.len())
    }

    fn step_tlb(&mut self) -> LevelResult {
        let access = self.in_progress.as_ref().unwrap();
        let address = access.address;
        let access_kind = access.kind;
        let page_size = self.config.page_size as u64;
        let vpn = address / page_size;
        let offset = address % page_size;

        match self.tlb.lookup(vpn) {
            Some(ppn) => {
                self.translated_address = Some(ppn * page_size + offset);
                self.cursor = Some(Cursor::Cache(0));
                LevelResult { level: LevelId::Tlb, outcome: LevelOutcome::Hit, address, access_kind, page_translation_outcome: None }
            }
            None => {
                self.walk = Some(WalkState { vpn, level: self.page_table.levels(), chain_index: self.resolve_page_walk_start_index() });
                self.cursor = Some(Cursor::Walk);
                LevelResult { level: LevelId::Tlb, outcome: LevelOutcome::Miss, address, access_kind, page_translation_outcome: None }
            }
        }
    }

    /// Checks whether this page table entry's own real address is already cached at this level.
    /// A level with no matching side just counts as a miss, same as a real access.
    fn walk_cache_lookup(&mut self, chain_index: usize, entry_address: u64, is_fetch: bool) -> LevelOutcome {
        let Some(slot) = self.levels[chain_index].slot_for(is_fetch) else {
            return LevelOutcome::Miss;
        };
        let (cache_config, exp, cache) = self.levels[chain_index].entry_mut(&slot);
        let block_size = cache_config.block_size.max(1) as u64;
        let num_sets = ((1u64 << *exp) / cache_config.associativity.max(1) as u64).max(1);
        let tag = entry_address / block_size;
        let set_index = tag % num_sets;
        match cache.lookup(set_index, tag) {
            super::cache::LookupOutcome::Hit => LevelOutcome::Hit,
            super::cache::LookupOutcome::Miss => {
                cache.insert(set_index, tag, false);
                LevelOutcome::Miss
            }
        }
    }

    fn resolve_leaf_and_finish(&mut self, vpn: u64) -> LevelOutcome {
        match self.page_table.leaf_entry(vpn) {
            Some(entry) if entry.present => {
                self.tlb.insert(vpn, entry.points_to_or_ppn);
                self.walk = None;
                self.translated_address = Some(self.physical_address_for(entry.points_to_or_ppn));
                self.cursor = Some(Cursor::Cache(0));
                LevelOutcome::Hit
            }
            _ => {
                let (frame, evict) = self.frames.allocate(vpn);
                self.walk = None;
                if let Some(e) = &evict {
                    // That page's mapping is now gone, so mark it not present and drop its TLB entry too.
                    self.page_table.set_leaf_absent(e.vpn);
                    self.tlb.invalidate(e.vpn);
                    // Drop any leftover cached lines for this frame, or they could wrongly count as hits for the new page.
                    self.invalidate_physical_cache_lines_for_frame(e.frame);
                }
                let go_to_swap_out = matches!(&evict, Some(e) if e.dirty);
                self.pending_fault = Some(PendingFault { vpn, frame, evict });
                self.cursor = Some(if go_to_swap_out { Cursor::SwapOut } else { Cursor::SwapIn });
                LevelOutcome::Miss
            }
        }
    }

    fn step_walk(&mut self) -> LevelResult {
        let walk = self.walk.as_ref().expect("step_walk called without walk state");
        let (vpn, level, chain_index) = (walk.vpn, walk.level, walk.chain_index);
        let access = self.in_progress.as_ref().unwrap();
        let (real_address, access_kind) = (access.address, access.kind);

        if chain_index >= self.levels.len() {
            let page_translation_outcome = if level == 1 {
                Some(self.resolve_leaf_and_finish(vpn))
            } else {
                self.page_table.resolve_intermediate(level, vpn);
                self.walk = Some(WalkState { vpn, level: level - 1, chain_index: self.resolve_page_walk_start_index() });
                self.cursor = Some(Cursor::Walk);
                None
            };
            let outcome = page_translation_outcome.unwrap_or(LevelOutcome::Hit);
            return LevelResult { level: LevelId::Ram, outcome, address: real_address, access_kind, page_translation_outcome };
        }

        let entry_address = self.page_table.entry_address(level, vpn);
        let cache_outcome = self.walk_cache_lookup(chain_index, entry_address, access_kind == AccessKind::Fetch);
        let mut page_translation_outcome = None;
        match cache_outcome {
            LevelOutcome::Hit => {
                if level == 1 {
                    page_translation_outcome = Some(self.resolve_leaf_and_finish(vpn));
                } else {
                    self.walk = Some(WalkState { vpn, level: level - 1, chain_index: self.resolve_page_walk_start_index() });
                    self.cursor = Some(Cursor::Walk);
                }
            }
            LevelOutcome::Miss => {
                self.walk.as_mut().unwrap().chain_index = chain_index + 1;
                self.cursor = Some(Cursor::Walk);
            }
        }
        LevelResult { level: LevelId::Cache(chain_index), outcome: cache_outcome, address: real_address, access_kind, page_translation_outcome }
    }

    fn write_disk_page(&mut self, vpn: u64, bytes: &[u8]) {
        let page_size = self.config.page_size as u64;
        let base = vpn * page_size;
        for (i, &b) in bytes.iter().enumerate() {
            self.disk.write_byte(base + i as u64, b);
        }
    }

    /// Bytes plus a populated flag for each word, so a caller can tell a word the program really wrote from one that just defaults to zero.
    fn read_disk_page(&mut self, vpn: u64) -> (Vec<u8>, Vec<bool>) {
        let page_size = self.config.page_size as usize;
        let base = vpn * page_size as u64;
        let bytes: Vec<u8> = (0..page_size).map(|i| self.disk.access_byte(base + i as u64)).collect();
        // Disk only knows whether a whole page has been written, so use that one flag for every word.
        let populated = vec![self.disk.is_populated(base); (page_size / 4).max(1)];
        (bytes, populated)
    }

    fn step_swap_out(&mut self) -> LevelResult {
        let fault = self.pending_fault.as_ref().expect("step_swap_out called without a pending fault");
        let evict = fault.evict.as_ref().expect("SwapOut is only reached when there is something dirty to evict");
        let (frame, evict_vpn) = (fault.frame, evict.vpn);
        let bytes = self.frames.read_page_bytes(frame);
        self.write_disk_page(evict_vpn, &bytes);
        self.cursor = Some(Cursor::SwapIn);
        let page_size = self.config.page_size as u64;
        LevelResult { level: LevelId::Disk, outcome: LevelOutcome::Hit, address: evict_vpn * page_size, access_kind: AccessKind::Store, page_translation_outcome: None }
    }

    fn step_swap_in(&mut self) -> LevelResult {
        let fault = self.pending_fault.take().expect("step_swap_in called without a pending fault");
        let (bytes, populated) = self.read_disk_page(fault.vpn);
        self.frames.write_page_bytes(fault.frame, &bytes, &populated);
        self.page_table.set_leaf_present(fault.vpn, fault.frame);
        self.tlb.insert(fault.vpn, fault.frame);
        self.translated_address = Some(self.physical_address_for(fault.frame));
        let access = self.in_progress.as_ref().unwrap();
        let address = access.address;
        self.cursor = Some(Cursor::Cache(0));
        LevelResult { level: LevelId::Disk, outcome: LevelOutcome::Hit, address, access_kind: AccessKind::Load, page_translation_outcome: None }
    }

    /// Reads the finished Load or Fetch's bytes from the real backing store, RAM or frames under VM, since cache levels never keep their own bytes.
    /// Uses access_byte so a real Load can turn an untouched already loaded cell into real data.
    fn read_load_bytes(&mut self, size: u8) -> Vec<u8> {
        let address = self.effective_address();
        if self.config.vm_enabled {
            let page_size = self.frames.page_size();
            (0..size)
                .map(|i| {
                    let a = address + i as u64;
                    self.frames.read_byte(a / page_size, a % page_size)
                })
                .collect()
        } else {
            (0..size).map(|i| self.ram.access_byte(address + i as u64)).collect()
        }
    }

    /// Touches the frames behind a Fetch under VM just to mark them as recently used.
    /// A Fetch never reads bytes, but without this a code only page would look idle and always get evicted first.
    fn touch_frames_for_fetch(&mut self, size: u8) {
        let address = self.effective_address();
        let page_size = self.frames.page_size();
        for i in 0..size {
            let a = address + i as u64;
            self.frames.read_byte(a / page_size, a % page_size);
        }
    }

    fn finish_with_load(&mut self, access: &MemoryAccess) {
        let load_bytes = match access.kind {
            AccessKind::Load => Some(self.read_load_bytes(access.size)),
            AccessKind::Fetch => {
                if self.config.vm_enabled {
                    self.touch_frames_for_fetch(access.size);
                }
                None
            }
            AccessKind::Store => None,
        };
        self.finished_result = Some(AccessResult { load_bytes });
    }

    fn write_store_bytes(&mut self, access: &MemoryAccess) {
        let address = self.effective_address();
        let bytes = access.store_bytes.clone().expect("a Store access always carries store_bytes");
        if self.config.vm_enabled {
            let page_size = self.frames.page_size();
            for (i, &b) in bytes.iter().enumerate() {
                let a = address + i as u64;
                self.frames.write_byte(a / page_size, a % page_size, b);
            }
        } else {
            for (i, &b) in bytes.iter().enumerate() {
                self.ram.write_byte(address + i as u64, b);
            }
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
            // This level has no cache for this kind of access, so skip it and move straight to the next level.
            self.cursor = Some(Cursor::Cache(index + 1));
            return self.step_cache(index + 1);
        };

        let effective_address = self.effective_address();
        let (cache_config, exp, cache) = self.levels[index].entry_mut(&slot);
        let block_size = cache_config.block_size.max(1) as u64;
        let write_policy = cache_config.write_policy;
        let write_allocate = cache_config.write_allocate;
        let num_sets = ((1u64 << *exp) / cache_config.associativity.max(1) as u64).max(1);
        let tag = effective_address / block_size;
        let set_index = tag % num_sets;

        let outcome = match cache.lookup(set_index, tag) {
            super::cache::LookupOutcome::Hit => {
                match access.kind {
                    AccessKind::Store if write_policy == WritePolicy::WriteBack => {
                        // A write back hit marks the line dirty and stops here, the write lands in the data store right away.
                        cache.mark_dirty(set_index, tag);
                        self.write_store_bytes(&access);
                        self.finished_result = Some(AccessResult { load_bytes: None });
                    }
                    AccessKind::Store => {
                        // A write through hit keeps going down through the remaining levels, the write happens at RAM.
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

        LevelResult { level: LevelId::Cache(index), outcome, address: access.address, access_kind: access.kind, page_translation_outcome: None }
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
        LevelResult { level: LevelId::Ram, outcome: LevelOutcome::Hit, address: access.address, access_kind: access.kind, page_translation_outcome: None }
    }

    pub fn tlb_info(&self) -> TlbInfo {
        TlbInfo { associativity: self.tlb.associativity(), num_sets: self.tlb.num_sets() }
    }

    pub fn tlb_sets(&self, start_set: u64, count: u32) -> Vec<TlbSetSnapshot> {
        let end = start_set.saturating_add(count as u64).min(self.tlb.num_sets());
        (start_set.min(self.tlb.num_sets())..end)
            .map(|set_index| TlbSetSnapshot { set_index, entries: self.tlb.snapshot_set(set_index) })
            .collect()
    }

    pub fn frame_snapshots(&self, start_frame: u64, count: u32) -> Vec<FrameContentSnapshot> {
        let end = start_frame.saturating_add(count as u64).min(self.frames.num_frames());
        (start_frame.min(self.frames.num_frames())..end)
            .map(|frame| {
                let snap = self.frames.snapshot(frame);
                let (words, populated) = if snap.valid {
                    let words = self
                        .frames
                        .read_page_bytes(frame)
                        .chunks(4)
                        .map(|chunk| {
                            let mut bytes = [0u8; 4];
                            bytes[..chunk.len()].copy_from_slice(chunk);
                            u32::from_le_bytes(bytes)
                        })
                        .collect();
                    (words, self.frames.read_page_populated(frame))
                } else {
                    (Vec::new(), Vec::new())
                };
                FrameContentSnapshot { frame, vpn: snap.vpn, valid: snap.valid, dirty: snap.dirty, words, populated }
            })
            .collect()
    }

    pub fn page_table_entries_under(&self, level: u8, parent_key: Option<u64>, start_index: u64, count: u64) -> Vec<super::page_table::PageTableEntrySnapshot> {
        self.page_table.entries_under(level, parent_key, start_index, count)
    }

    pub fn page_table_entries_under_count(&self, level: u8, parent_key: Option<u64>) -> u64 {
        self.page_table.entries_under_count(level, parent_key)
    }

    pub fn page_table_levels(&self) -> u8 {
        self.page_table.levels()
    }

    pub fn page_table_level_bits(&self) -> &[u32] {
        self.page_table.level_bits()
    }

    pub fn disk_pages(&self, start_vpn: u64, count: u32) -> Vec<RamBlockSnapshot> {
        let page_size = self.config.page_size as u64;
        (0..count as u64)
            .map(|i| {
                let vpn = start_vpn + i;
                let base = vpn * page_size;
                let words = self
                    .disk
                    .read_block(base, page_size as usize)
                    .chunks(4)
                    .map(|chunk| {
                        let mut bytes = [0u8; 4];
                        bytes[..chunk.len()].copy_from_slice(chunk);
                        u32::from_le_bytes(bytes)
                    })
                    .collect();
                RamBlockSnapshot { address: vpn, words, populated: self.disk.is_populated(base), dirty: self.disk.is_dirty(base) }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::{DiskConfig, FrameAllocationPolicy, InclusionPolicy, RamConfig, ReplacementPolicy, TlbConfig, TlbReplacementPolicy};
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
            page_size: 4096,
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
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
                latency: 1,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
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
        // A real session's disk is one block per whole page under VM, match that here.
        let disk_block_size = (config.page_size as usize).max(1);
        MemoryHierarchySimulator::new(config, ram, DataStore::new(disk_block_size), 0)
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
    fn a_cache_level_configured_for_fifo_evicts_by_insertion_order_not_recency() {
        let mut cfg = cache_config();
        cfg.associativity = 2;
        cfg.replacement = ReplacementPolicy::Fifo;
        let mut mem = sim(one_level_config_with(false, cfg));

        for addr in [0u64, 4u64, 0u64] {
            mem.begin_access(load(addr));
            while !mem.is_access_finished() {
                mem.step();
            }
            mem.take_result();
        }
        // Both addresses are now resident, and address 0 was just hit again.
        // A third address forces an eviction, FIFO must evict address 0's line even though it was touched most recently.
        mem.begin_access(load(8));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let first = mem.begin_access(load(0));
        assert_eq!(first.level, LevelId::Cache(0));
        assert_eq!(first.outcome, LevelOutcome::Miss, "FIFO should have evicted address 0's line, not address 4's");
    }

    #[test]
    fn a_tlb_configured_for_fifo_evicts_by_insertion_order_not_recency() {
        let mut cfg = vm_config(1);
        cfg.tlb.entries = 2;
        cfg.tlb.associativity = 2; // 1 set, so every vpn collides there
        cfg.tlb.replacement = TlbReplacementPolicy::Fifo;
        let mut mem = sim(cfg);

        // The page size is 4 here, so vpn 0 covers addresses 0 to 3 and vpn 1 covers 4 to 7.
        for addr in [0u64, 4u64, 0u64] {
            mem.begin_access(load(addr));
            while !mem.is_access_finished() {
                mem.step();
            }
            mem.take_result();
        }
        // Both vpn 0 and vpn 1 are now in the TLB, and vpn 0 was just hit again.
        // A third page forces an eviction, FIFO must evict vpn 0's entry, not vpn 1's.
        mem.begin_access(load(8)); // vpn 2
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let first = mem.begin_access(load(0));
        assert_eq!(first.level, LevelId::Tlb);
        assert_eq!(first.outcome, LevelOutcome::Miss, "FIFO should have evicted vpn 0's TLB entry, not vpn 1's");
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

        // Prime the line, the first store misses, gets added to the cache, and ends up written to RAM.
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // A second store to the same address now hits at L1, a write back hit must finish right there with just one result.
        let result = mem.begin_access(store(0x10000000, vec![0xAA, 0xBB, 0xCC, 0xDD]));
        assert_eq!(result.level, LevelId::Cache(0));
        assert_eq!(result.outcome, LevelOutcome::Hit);
        assert!(mem.is_access_finished());
        mem.take_result();

        // The write back hit must still have applied the write straight to RAM, since that is where the data really ends up.
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

        // Prime the line, the first store misses, gets added to the cache clean, and writes to RAM.
        mem.begin_access(store(0x10000000, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // A second store hits at L1, but a write through hit must not finish there, it keeps going down to RAM.
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

        // A TLB miss now triggers a real page walk before the data cache is checked.
        // The walk goes straight to RAM, faults since this is a fresh page, and swaps in from disk first.
        let second = mem.step();
        assert_eq!(second.level, LevelId::Ram, "the page-table lookup reaches RAM before any data cache level");

        let mut saw_cache = false;
        while !mem.is_access_finished() {
            if mem.step().level == LevelId::Cache(0) {
                saw_cache = true;
            }
        }
        assert!(saw_cache, "the walk must eventually resolve and let the access reach the data cache");
    }

    #[test]
    fn vm_disabled_never_produces_a_tlb_level_result() {
        let mut mem = sim(one_level_config(false));
        let first = mem.begin_access(load(0x10000000));
        assert_ne!(first.level, LevelId::Tlb);
    }

    #[test]
    fn fetch_walks_the_hierarchy_with_the_same_shape_as_an_equivalent_load() {
        // A fresh Fetch must miss at L1 then hit at RAM, just like a fresh Load would.
        // The only difference is the label on the result.
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
            page_size: 4096,
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
                page_walk_start: "RAM".into(),
                entries: 4,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
                latency: 1,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    #[test]
    fn fetch_skips_a_harvard_level_with_no_instruction_side_configured() {
        // L1 has only a data cache, no instruction cache.
        // A Fetch must skip straight past it to L2, the same way a Store skips a level with no matching cache.
        let mut mem = sim(harvard_d_only_then_unified_hierarchy_config());
        let first = mem.begin_access(fetch(0x10000000));
        assert_eq!(first.level, LevelId::Cache(1), "L1 (d-only) must be skipped entirely for a fetch");
        assert_eq!(first.outcome, LevelOutcome::Miss);
    }

    fn vm_harvard_config() -> HierarchyConfig {
        let mut cfg = vm_config(1);
        cfg.hierarchy = vec![Level {
            id: "l1".into(),
            harvard: true,
            unified: None,
            i: Some(CacheEntry { exp: 8, config: cache_config() }),
            d: Some(CacheEntry { exp: 8, config: cache_config() }),
            inclusion: InclusionPolicy::Nine,
            bus_width: 32,
        }];
        cfg.tlb.page_walk_start = "l1".into();
        cfg
    }

    #[test]
    fn a_fetchs_page_walk_probes_the_instruction_side_of_a_harvard_level_not_the_data_side() {
        let mut mem = sim(vm_harvard_config());
        mem.begin_access(fetch(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let i_info = mem.cache_level_info(0, CacheSide::Instruction).unwrap();
        let d_info = mem.cache_level_info(0, CacheSide::Data).unwrap();
        let count_valid = |side: CacheSide, info: &CacheLevelInfo| -> usize {
            (0..info.num_sets).map(|s| mem.cache_set_ways(0, side, s, info).iter().filter(|w| w.valid).count()).sum()
        };

        assert!(count_valid(CacheSide::Instruction, &i_info) > 0, "the fetch's own page walk must land in the instruction side");
        assert_eq!(count_valid(CacheSide::Data, &d_info), 0, "a fetch's page walk must never touch the data side");
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
        assert_eq!(info.words_per_line, 1); // 4 bytes makes one word of 4 bytes
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

        // This address with block size 4 gives tag 0x4000000, landing in set 0.
        let set0 = sets.iter().find(|s| s.set_index == 0).unwrap();
        let way = set0.ways.iter().find(|w| w.valid).expect("the stored line must be present");
        assert_eq!(way.tag, 0x10000000 / 4);
        assert_eq!(way.words, vec![u32::from_le_bytes([0x11, 0x22, 0x33, 0x44])]);
    }

    #[test]
    fn cache_sets_reflects_a_stored_lines_real_words_under_vm() {
        // Under VM the frame table is the real store, so the cache inspector must read words from there.
        // Otherwise every line would show as zero no matter what was really stored.
        let mut mem = sim(vm_config(1));
        mem.begin_access(store(0, vec![0x11, 0x22, 0x33, 0x44]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let info = mem.cache_level_info(0, CacheSide::Unified).unwrap();
        let sets = mem.cache_sets(0, CacheSide::Unified, 0, info.num_sets as u32).unwrap();
        let way = sets.iter().flat_map(|s| &s.ways).find(|w| w.valid).expect("the stored line must be present");
        assert_eq!(way.words, vec![u32::from_le_bytes([0x11, 0x22, 0x33, 0x44])]);
    }

    #[test]
    fn cache_sets_does_not_overflow_reconstructing_words_for_a_dirty_seeded_phantom_line() {
        // A made up dirty line's tag is just a large random number, not a real address divided by the block size.
        // Multiplying that huge tag back used to overflow and crash the whole app on an untouched set.
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
        // A real store of this value lays out its bytes with the smallest one first.
        // The snapshot's words should rebuild the exact same value, not one with the bytes swapped.
        let mut mem = sim(one_level_config(false));
        mem.begin_access(store(0x10000000, vec![0x78, 0x56, 0x34, 0x12]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let blocks = mem.ram_blocks(0x10000000, 1);
        assert_eq!(blocks[0].words, vec![0x12345678]);
    }

    fn vm_config(page_table_levels: u8) -> HierarchyConfig {
        let mut cfg = one_level_config(true);
        cfg.page_size = 4; // tiny, so tests can use small addresses
        cfg.ram.frames = 4;
        cfg.ram.page_table_levels = page_table_levels;
        cfg.tlb.page_walk_start = "RAM".into();
        cfg
    }

    #[test]
    fn frame_table_is_built_from_frames_times_page_size_not_a_byte_size() {
        let mut cfg = vm_config(1);
        cfg.page_size = 8;
        cfg.ram.frames = 3;
        let mem = sim(cfg);
        assert_eq!(mem.frames.num_frames(), 3);
    }

    #[test]
    fn page_table_levels_is_clamped_to_the_available_vpn_bits() {
        // This page size leaves 20 address bits available, asking for 50 levels must not create any with 0 bits.
        let mut cfg = vm_config(50);
        cfg.page_size = 4096;
        let mem = sim(cfg);
        assert_eq!(mem.page_table_levels(), 20);
    }

    #[test]
    fn page_table_level_bits_reports_the_leaf_favored_split() {
        let mut cfg = vm_config(3);
        cfg.page_size = 4096; // 20 VPN bits on RV32I, split 7/7/6 favoring the leaf
        let mem = sim(cfg);
        assert_eq!(mem.page_table_level_bits(), &[7, 7, 6]);
    }

    #[test]
    fn a_tlb_hit_skips_the_walk_entirely() {
        let mut mem = sim(vm_config(1));
        mem.begin_access(store(0, vec![1, 2, 3, 4]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // On the second access to the same page, the TLB now has the mapping.
        let first = mem.begin_access(load(0));
        assert_eq!(first.level, LevelId::Tlb);
        assert_eq!(first.outcome, LevelOutcome::Hit);
        let second = mem.step();
        assert_eq!(second.level, LevelId::Cache(0), "must go straight to the physical chain, no Walk step");
    }

    #[test]
    fn walk_lookups_are_cached_and_shared_across_vpns_with_the_same_prefix() {
        // Point the walk start at the l1 cache instead of the usual RAM shortcut, so the walk lookups actually reach the cache.
        let mut cfg = vm_config(2);
        cfg.tlb.page_walk_start = "l1".into();
        // Give the cache far more sets than the default so unrelated lookups made along the way do not land in the same set.
        // Otherwise they could evict the root level lookup before the second walk gets a chance to hit it.
        cfg.hierarchy[0].unified.as_mut().unwrap().exp = 8; // 256 sets
        let mut mem = sim(cfg);

        // The first walk goes to vpn 0, the TLB misses, then the root level lookup in l1 must miss too since nothing is cached yet.
        let tlb = mem.begin_access(load(0));
        assert_eq!(tlb.level, LevelId::Tlb);
        assert_eq!(tlb.outcome, LevelOutcome::Miss);

        let root_lookup = mem.step();
        assert_eq!(root_lookup.level, LevelId::Cache(0));
        assert_eq!(root_lookup.outcome, LevelOutcome::Miss, "first walk's root-level PTE lookup must miss the walk cache");

        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // The second walk goes to vpn 1, which shares the same top level prefix as vpn 0 but is still a distinct page.
        // Its root level lookup must now hit the walk cache since the first walk already cached that address.
        let tlb2 = mem.begin_access(load(4));
        assert_eq!(tlb2.level, LevelId::Tlb);
        assert_eq!(tlb2.outcome, LevelOutcome::Miss);

        let root_lookup2 = mem.step();
        assert_eq!(root_lookup2.level, LevelId::Cache(0));
        assert_eq!(
            root_lookup2.outcome,
            LevelOutcome::Hit,
            "second walk shares the first walk's root-level PTE, so it must hit the cached synthetic address"
        );
    }

    #[test]
    fn a_fresh_page_faults_and_ends_up_readable() {
        let mut mem = sim(vm_config(1));
        mem.begin_access(store(0, vec![0xAA, 0xBB, 0xCC, 0xDD]));
        let mut saw_disk = false;
        while !mem.is_access_finished() {
            let result = mem.step();
            if result.level == LevelId::Disk {
                saw_disk = true;
            }
        }
        assert!(saw_disk, "a brand new page must fault and swap in from disk");
        mem.take_result();

        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![0xAA, 0xBB, 0xCC, 0xDD]));
    }

    #[test]
    fn preload_ram_under_vm_lands_where_a_real_page_fault_can_actually_reach_it() {
        // Under VM a fresh page always comes from disk, so a preload that landed in RAM would never be reachable.
        let mut mem = sim(vm_config(1));
        mem.preload_ram(0, &[0xEF, 0xBE, 0xAD, 0xDE]);

        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![0xEF, 0xBE, 0xAD, 0xDE]), "the preloaded bytes must survive the very first page fault");
    }

    #[test]
    fn a_page_survives_an_evict_then_refault_round_trip_with_all_its_real_content() {
        // A small page size and only 2 frames means a third page evicts vpn 0's frame, so touching vpn 0 again forces a refault.
        let mut cfg = vm_config(1);
        cfg.page_size = 16;
        cfg.ram.frames = 2;
        let mut mem = sim(cfg);

        // Store only to the second word of vpn 0's page.
        mem.begin_access(store(4, vec![9, 8, 7, 6]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // Touch two more pages, filling the second frame, then evicting vpn 0's frame as least recently used for a third page.
        mem.begin_access(load(16)); // vpn 1
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();
        mem.begin_access(load(32)); // vpn 2, evicts vpn 0's frame
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // Faulting vpn 0 back in must come back from disk with its real stored word intact, not reverted to zero.
        mem.begin_access(load(4));
        while !mem.is_access_finished() {
            mem.step();
        }
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![9, 8, 7, 6]), "the word actually stored before the eviction must survive the disk round trip");
    }

    #[test]
    fn a_page_fault_only_marks_the_words_actually_written_as_populated() {
        // A bigger page than the default, so a single store only touches one of several words in it.
        let mut cfg = vm_config(1);
        cfg.page_size = 16;
        cfg.ram.frames = 2;
        let mut mem = sim(cfg);

        // Storing to the second word of a brand new page must fault, swap in an empty page from disk, then write only that word.
        mem.begin_access(store(4, vec![9, 8, 7, 6]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let snaps = mem.frame_snapshots(0, 2);
        let frame = snaps.iter().find(|s| s.valid).expect("the page fault must have landed in some frame");
        assert_eq!(frame.populated, vec![false, true, false, false], "only the word actually stored to must be populated");
        assert_eq!(frame.words[1], u32::from_le_bytes([9, 8, 7, 6]));
    }

    #[test]
    fn evicting_a_frame_invalidates_the_stale_physical_cache_line_it_leaves_behind() {
        // With a split cache and only 1 frame, a second page immediately evicts the only frame.
        // A leftover cached line from the evicted page must not linger once a different page takes that frame.
        let mut cfg = vm_config(1);
        cfg.ram.frames = 1;
        cfg.hierarchy[0].harvard = true;
        cfg.hierarchy[0].unified = None;
        cfg.hierarchy[0].i = Some(CacheEntry { exp: 1, config: cache_config() }); // block_size 4, 2 sets
        cfg.hierarchy[0].d = Some(CacheEntry { exp: 1, config: cache_config() });
        let mut mem = sim(cfg);

        // vpn 0's page is fetched, caching its only frame into L1i.
        mem.begin_access(fetch(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();
        let before = mem.cache_sets(0, CacheSide::Instruction, 0, 2).unwrap();
        assert!(before.iter().any(|s| s.ways.iter().any(|w| w.valid && w.tag == 0)), "the fetch must have cached physical address 0");

        // vpn 1's page needs the only frame, evicting vpn 0's page and reusing that same frame for a different page.
        mem.begin_access(store(4, vec![1, 2, 3, 4]));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let after = mem.cache_sets(0, CacheSide::Instruction, 0, 2).unwrap();
        assert!(
            !after.iter().any(|s| s.ways.iter().any(|w| w.valid && w.tag == 0)),
            "the evicted page's stale L1i line for that physical address must be dropped, not left aliasing the new page"
        );
    }

    #[test]
    fn evicting_a_clean_frame_never_touches_disk_for_the_writeback() {
        let mut mem = sim(vm_config(1)); // 4 frames, page_size 4
        // Fill all 4 frames with fresh loads, so every frame stays clean.
        for page in 0..4u64 {
            mem.begin_access(load(page * 4));
            while !mem.is_access_finished() {
                mem.step();
            }
            mem.take_result();
        }
        // A 5th page forces an eviction, and since every resident frame is clean, the evicted one skips straight to swap in.
        // That means only one disk step happens, not two.
        mem.begin_access(load(4 * 4));
        let mut disk_steps = 0;
        while !mem.is_access_finished() {
            if mem.step().level == LevelId::Disk {
                disk_steps += 1;
            }
        }
        assert_eq!(disk_steps, 1, "a clean eviction must not write back to disk");
    }

    #[test]
    fn a_clean_eviction_correctly_invalidates_the_page_table_and_tlb_entries_it_leaves_behind() {
        // A clean eviction must mark the evicted page absent and drop its TLB entry.
        // Otherwise a later access silently reads whatever page now occupies that frame, instead of faulting back in from disk.
        let mut mem = sim(vm_config(1)); // 4 frames, page_size 4

        // Each page gets its own disk content, written directly so it is known ahead of any access.
        mem.disk.write_byte(0 * 4, 0xAA);
        mem.disk.write_byte(4 * 4, 0xBB); // the page that will steal page 0's frame

        // Load page 0 first, so it becomes the least recently used frame once the others load after it.
        // This is a Load, so the frame stays clean.
        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        let first = mem.take_result();
        assert_eq!(first.load_bytes, Some(vec![0xAA, 0, 0, 0]));

        for page in 1..4u64 {
            mem.begin_access(load(page * 4));
            while !mem.is_access_finished() {
                mem.step();
            }
            mem.take_result();
        }

        // A 5th page evicts page 0's clean frame as least recently used.
        // Confirm it is really a clean eviction with only one disk step and that the new page's content is correct.
        mem.begin_access(load(4 * 4));
        let mut disk_steps = 0;
        while !mem.is_access_finished() {
            if mem.step().level == LevelId::Disk {
                disk_steps += 1;
            }
        }
        assert_eq!(disk_steps, 1, "the eviction backing this fault must still be clean");
        let evictor = mem.take_result();
        assert_eq!(evictor.load_bytes, Some(vec![0xBB, 0, 0, 0]));

        // Accessing page 0 again must fault again, since its frame now holds page 4's data.
        // It must correctly read page 0's original bytes back from disk, not page 4's leftover bytes.
        let refault = mem.begin_access(load(0));
        assert_eq!(refault.level, LevelId::Tlb);
        assert_eq!(refault.outcome, LevelOutcome::Miss, "the TLB entry for the evicted page must have been invalidated");
        let mut saw_disk_again = false;
        while !mem.is_access_finished() {
            if mem.step().level == LevelId::Disk {
                saw_disk_again = true;
            }
        }
        assert!(saw_disk_again, "the evicted page must genuinely fault again, not resolve through a stale mapping");
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![0xAA, 0, 0, 0]), "must read page 0's real original bytes, not page 4's");
    }

    #[test]
    fn a_fresh_page_fault_reports_a_miss_at_the_moment_of_translation() {
        let mut mem = sim(vm_config(1));
        let mut saw_page_translation_miss = false;
        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            let result = mem.step();
            if let Some(outcome) = result.page_translation_outcome {
                assert_eq!(outcome, LevelOutcome::Miss, "the very first access to any page must fault");
                saw_page_translation_miss = true;
            }
        }
        assert!(saw_page_translation_miss, "no step reported a page translation outcome for a fresh page");
    }

    #[test]
    fn re_accessing_an_already_mapped_page_reports_a_hit_at_the_moment_of_translation() {
        // Accessing the same address again normally hits the TLB and never walks the page table again.
        // A TLB with only one entry forces a real walk, while the page stays mapped.
        let mut cfg = vm_config(1);
        cfg.tlb.entries = 1;
        cfg.tlb.associativity = 1;
        let mut mem = sim(cfg);

        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // Touch a different page so the TLB with only one entry evicts vpn 0's mapping without evicting its frame.
        mem.begin_access(load(4));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let mut saw_page_translation_hit = false;
        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            let result = mem.step();
            if let Some(outcome) = result.page_translation_outcome {
                assert_eq!(outcome, LevelOutcome::Hit, "the page is already mapped from the first access");
                saw_page_translation_hit = true;
            }
        }
        assert!(saw_page_translation_hit, "no step reported a page translation outcome for an already mapped page");
    }

    #[test]
    fn a_page_translation_resolved_via_a_cached_walk_lookup_is_still_recorded() {
        // Route the walk through l1 instead of the usual RAM shortcut, so a leaf lookup can hit the cache.
        // A TLB with only one entry forces a real walk, and a wide cache keeps the first walk's line from being evicted.
        let mut cfg = vm_config(1);
        cfg.tlb.page_walk_start = "l1".into();
        cfg.tlb.entries = 1;
        cfg.tlb.associativity = 1;
        cfg.hierarchy[0].unified.as_mut().unwrap().exp = 8;
        let mut mem = sim(cfg);

        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        // Touch a different page so the TLB with only one entry evicts vpn 0's mapping without evicting its frame.
        mem.begin_access(load(4));
        while !mem.is_access_finished() {
            mem.step();
        }
        mem.take_result();

        let mut saw_cache_hit_page_translation = false;
        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            let result = mem.step();
            if matches!(result.level, LevelId::Cache(_)) && result.outcome == LevelOutcome::Hit && result.page_translation_outcome.is_some() {
                saw_cache_hit_page_translation = true;
            }
        }
        assert!(saw_cache_hit_page_translation, "a page translation resolved via a cached walk lookup must still be recorded");
    }

    #[test]
    fn evicting_a_dirty_frame_writes_it_back_before_swapping_in_the_new_page() {
        let mut mem = sim(vm_config(1));
        for page in 0..4u64 {
            mem.begin_access(store(page * 4, vec![page as u8; 4]));
            while !mem.is_access_finished() {
                mem.step();
            }
            mem.take_result();
        }
        // Force page 0's frame to become the least recently used one.
        for page in 1..4u64 {
            mem.begin_access(load(page * 4));
            while !mem.is_access_finished() {
                mem.step();
            }
            mem.take_result();
        }

        mem.begin_access(load(4 * 4)); // a 5th page, evicts page 0's dirty frame
        let mut disk_steps = 0;
        while !mem.is_access_finished() {
            if mem.step().level == LevelId::Disk {
                disk_steps += 1;
            }
        }
        assert_eq!(disk_steps, 2, "a dirty eviction must write back, then swap in");
        mem.take_result();

        // Page 0 must now be readable from disk with its original content, through a fresh page fault.
        mem.begin_access(load(0));
        while !mem.is_access_finished() {
            mem.step();
        }
        let result = mem.take_result();
        assert_eq!(result.load_bytes, Some(vec![0u8; 4]));
    }

    #[test]
    fn a_multi_level_page_table_walks_once_per_level_on_a_full_miss() {
        let mut mem = sim(vm_config(3));
        mem.begin_access(load(0));
        let mut ram_steps = 0;
        while !mem.is_access_finished() {
            if mem.step().level == LevelId::Ram {
                ram_steps += 1;
            }
        }
        // 3 page table levels each fall through to RAM once, plus the final data access's own RAM step.
        // The one cache level still misses on a cold access, so it also reaches RAM once more.
        assert_eq!(ram_steps, 4, "3 page-table levels plus the real data access must each reach RAM once");
    }

    #[test]
    fn a_fetch_walks_the_same_way_a_load_does_under_vm() {
        let mut fetch_mem = sim(vm_config(1));
        let mut load_mem = sim(vm_config(1));

        let mut fetch_levels = vec![];
        fetch_mem.begin_access(fetch(0));
        while !fetch_mem.is_access_finished() {
            fetch_levels.push(fetch_mem.step().level);
        }

        let mut load_levels = vec![];
        load_mem.begin_access(load(0));
        while !load_mem.is_access_finished() {
            load_levels.push(load_mem.step().level);
        }

        assert_eq!(fetch_levels, load_levels, "fetch needs no special-casing, it shares every step with load");
    }
}
