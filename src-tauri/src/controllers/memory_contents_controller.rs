// Commands used by the memory inspection panels to read the running simulation's registers, cache snapshots, and RAM blocks.
// Session setup and the live event stream live in a separate file.
use serde::Serialize;

use crate::models::simulation::memory::hierarchy::{
    CacheLevelInfo, CacheLineRowSnapshot, CacheSetSnapshot, CacheSide, FrameContentSnapshot, FrameLineRowSnapshot, RamBlockSnapshot, TlbInfo,
    TlbSetSnapshot,
};
use crate::models::simulation::memory::page_table::PageTableEntrySnapshot;
use crate::models::simulation::RegisterId;

use super::simulation_session_controller::SessionState;

#[derive(Debug, Serialize)]
pub struct RegisterSnapshot {
    pub int: Vec<u64>,
    pub fp: Vec<f64>,
}

fn snapshot_registers(state: &SessionState) -> Result<RegisterSnapshot, String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    let int = (0..32u8).map(|i| session.register(RegisterId(i))).collect();
    let fp = (0..32u8).map(|i| session.fp_register(RegisterId(i))).collect();
    Ok(RegisterSnapshot { int, fp })
}

#[tauri::command]
pub fn get_registers(state: tauri::State<SessionState>) -> Result<RegisterSnapshot, String> {
    snapshot_registers(&state)
}

fn parse_cache_side(side: &str) -> Result<CacheSide, String> {
    match side {
        "unified" => Ok(CacheSide::Unified),
        "i" => Ok(CacheSide::Instruction),
        "d" => Ok(CacheSide::Data),
        other => Err(format!("unknown cache side '{other}'")),
    }
}

fn cache_level_info(state: &SessionState, level_index: usize, side: String) -> Result<CacheLevelInfo, String> {
    let cache_side = parse_cache_side(&side)?;
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    session
        .memory()
        .cache_level_info(level_index, cache_side)
        .ok_or_else(|| format!("level {level_index} has no {side} cache"))
}

#[tauri::command]
pub fn get_cache_level_info(state: tauri::State<SessionState>, level_index: usize, side: String) -> Result<CacheLevelInfo, String> {
    cache_level_info(&state, level_index, side)
}

fn cache_sets(state: &SessionState, level_index: usize, side: String, start_set: u64, count: u32) -> Result<Vec<CacheSetSnapshot>, String> {
    // Limits how many sets one call can return, the same idea as the RAM block limit below.
    // A cache level can still have far more sets in total, the frontend fetches those in pages instead.
    const MAX_COUNT: u32 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let cache_side = parse_cache_side(&side)?;
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    session
        .memory()
        .cache_sets(level_index, cache_side, start_set, count)
        .ok_or_else(|| format!("level {level_index} has no {side} cache"))
}

#[tauri::command]
pub fn get_cache_sets(
    state: tauri::State<SessionState>,
    level_index: usize,
    side: String,
    start_set: u64,
    count: u32,
) -> Result<Vec<CacheSetSnapshot>, String> {
    cache_sets(&state, level_index, side, start_set, count)
}

fn cache_lines(state: &SessionState, level_index: usize, side: String, start_line: u64, count: u32) -> Result<Vec<CacheLineRowSnapshot>, String> {
    const MAX_COUNT: u32 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let cache_side = parse_cache_side(&side)?;
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    session
        .memory()
        .cache_lines(level_index, cache_side, start_line, count)
        .ok_or_else(|| format!("level {level_index} has no {side} cache"))
}

#[tauri::command]
pub fn get_cache_lines(
    state: tauri::State<SessionState>,
    level_index: usize,
    side: String,
    start_line: u64,
    count: u32,
) -> Result<Vec<CacheLineRowSnapshot>, String> {
    cache_lines(&state, level_index, side, start_line, count)
}

fn snapshot_ram_blocks(state: &SessionState, start_address: u64, count: u32) -> Result<Vec<RamBlockSnapshot>, String> {
    const MAX_COUNT: u32 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().ram_blocks(start_address, count))
}

#[tauri::command]
pub fn get_ram_blocks(state: tauri::State<SessionState>, start_address: u64, count: u32) -> Result<Vec<RamBlockSnapshot>, String> {
    snapshot_ram_blocks(&state, start_address, count)
}

fn tlb_info(state: &SessionState) -> Result<TlbInfo, String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().tlb_info())
}

#[tauri::command]
pub fn get_tlb_info(state: tauri::State<SessionState>) -> Result<TlbInfo, String> {
    tlb_info(&state)
}

fn tlb_sets(state: &SessionState, start_set: u64, count: u32) -> Result<Vec<TlbSetSnapshot>, String> {
    const MAX_COUNT: u32 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().tlb_sets(start_set, count))
}

#[tauri::command]
pub fn get_tlb_sets(state: tauri::State<SessionState>, start_set: u64, count: u32) -> Result<Vec<TlbSetSnapshot>, String> {
    tlb_sets(&state, start_set, count)
}

fn frame_snapshots(state: &SessionState, start_frame: u64, count: u32) -> Result<Vec<FrameContentSnapshot>, String> {
    const MAX_COUNT: u32 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().frame_snapshots(start_frame, count))
}

#[tauri::command]
pub fn get_frames(state: tauri::State<SessionState>, start_frame: u64, count: u32) -> Result<Vec<FrameContentSnapshot>, String> {
    frame_snapshots(&state, start_frame, count)
}

fn frame_lines(state: &SessionState, start_line: u64, count: u32) -> Result<Vec<FrameLineRowSnapshot>, String> {
    const MAX_COUNT: u32 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().frame_lines(start_line, count))
}

#[tauri::command]
pub fn get_frame_lines(state: tauri::State<SessionState>, start_line: u64, count: u32) -> Result<Vec<FrameLineRowSnapshot>, String> {
    frame_lines(&state, start_line, count)
}

fn page_table_entries(state: &SessionState, level: u8, parent_key: Option<u64>, start_index: u64, count: u64) -> Result<Vec<PageTableEntrySnapshot>, String> {
    const MAX_COUNT: u64 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().page_table_entries_under(level, parent_key, start_index, count))
}

#[tauri::command]
pub fn get_page_table_entries(
    state: tauri::State<SessionState>,
    level: u8,
    parent_key: Option<u64>,
    start_index: u64,
    count: u64,
) -> Result<Vec<PageTableEntrySnapshot>, String> {
    page_table_entries(&state, level, parent_key, start_index, count)
}

fn page_table_entry_count(state: &SessionState, level: u8, parent_key: Option<u64>) -> Result<u64, String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().page_table_entries_under_count(level, parent_key))
}

#[tauri::command]
pub fn get_page_table_entry_count(state: tauri::State<SessionState>, level: u8, parent_key: Option<u64>) -> Result<u64, String> {
    page_table_entry_count(&state, level, parent_key)
}

#[tauri::command]
pub fn get_page_table_levels(state: tauri::State<SessionState>) -> Result<u8, String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().page_table_levels())
}

#[tauri::command]
pub fn get_page_table_level_bits(state: tauri::State<SessionState>) -> Result<Vec<u32>, String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().page_table_level_bits().to_vec())
}

fn disk_pages(state: &SessionState, start_vpn: u64, count: u32) -> Result<Vec<RamBlockSnapshot>, String> {
    const MAX_COUNT: u32 = 512;
    if count > MAX_COUNT {
        return Err(format!("count {count} exceeds the maximum of {MAX_COUNT}"));
    }
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    Ok(session.memory().disk_pages(start_vpn, count))
}

#[tauri::command]
pub fn get_disk_pages(state: tauri::State<SessionState>, start_vpn: u64, count: u32) -> Result<Vec<RamBlockSnapshot>, String> {
    disk_pages(&state, start_vpn, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::Base;
    use crate::models::simulation::memory::config::{
        CacheConfig, CacheEntry, DiskConfig, FrameAllocationPolicy, HierarchyConfig, InclusionPolicy, Level, RamConfig, ReplacementPolicy, TlbConfig,
        TlbReplacementPolicy, WritePolicy,
    };
    use crate::controllers::simulation_session_controller::build_session;

    fn empty_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            page_size: 4096,
            hierarchy: vec![],
            tlb: TlbConfig { page_walk_start: "RAM".into(), entries: 4, associativity: 4, replacement: TlbReplacementPolicy::Lru, latency: 1 },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    fn empty_codegen() -> crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
        crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
            layout: crate::models::interpreter::riscv::codegen::ir::MemoryLayout {
                text_base: 0x1000,
                data_base: 0x2000,
                labels: std::collections::HashMap::new(),
                initial_data: vec![],
            },
            entries: vec![],
        }
    }

    #[test]
    fn snapshot_registers_errors_with_no_active_session() {
        let state = SessionState::default();
        let result = snapshot_registers(&state);
        assert_eq!(result.unwrap_err(), "no active simulation session");
    }

    #[test]
    fn cache_level_info_errors_with_no_active_session() {
        let state = SessionState::default();
        let result = cache_level_info(&state, 0, "unified".to_string());
        assert_eq!(result.unwrap_err(), "no active simulation session");
    }

    #[test]
    fn cache_sets_errors_with_no_active_session() {
        let state = SessionState::default();
        let result = cache_sets(&state, 0, "unified".to_string(), 0, 10);
        assert_eq!(result.unwrap_err(), "no active simulation session");
    }

    #[test]
    fn cache_sets_rejects_a_count_over_the_max() {
        let state = SessionState::default();
        let result = cache_sets(&state, 0, "unified".to_string(), 0, 513);
        assert!(result.unwrap_err().contains("exceeds the maximum"));
    }

    #[test]
    fn snapshot_ram_blocks_errors_with_no_active_session() {
        let state = SessionState::default();
        let result = snapshot_ram_blocks(&state, 0, 1);
        assert_eq!(result.unwrap_err(), "no active simulation session");
    }

    fn oversized_cache_hierarchy_config() -> HierarchyConfig {
        // This works out to over a million sets, far more than a single call would ever request.
        // That is fine, since fetching the level's info separately from fetching its sets in pages avoids that problem.
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            page_size: 4096,
            hierarchy: vec![Level {
                id: "l1".into(),
                harvard: false,
                unified: Some(CacheEntry {
                    exp: 20,
                    config: CacheConfig {
                        block_size: 64,
                        associativity: 1,
                        replacement: ReplacementPolicy::Lru,
                        write_policy: WritePolicy::WriteBack,
                        write_allocate: true,
                        hit_time: 1,
                        dirty_percent: 0,
                    },
                }),
                i: None,
                d: None,
                inclusion: InclusionPolicy::Nine,
                bus_width: 32,
            }],
            tlb: TlbConfig { page_walk_start: "RAM".into(), entries: 4, associativity: 4, replacement: TlbReplacementPolicy::Lru, latency: 1 },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    #[test]
    fn cache_level_info_reports_a_huge_num_sets_without_erroring() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), oversized_cache_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);

        let info = cache_level_info(&state, 0, "unified".to_string()).unwrap();
        assert_eq!(info.num_sets, 1 << 20);
    }

    #[test]
    fn cache_sets_pages_through_a_huge_level_without_requesting_it_all_at_once() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), oversized_cache_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);

        let sets = cache_sets(&state, 0, "unified".to_string(), 500_000, 10).unwrap();
        assert_eq!(sets.len(), 10);
        assert_eq!(sets[0].set_index, 500_000);
        assert_eq!(sets[9].set_index, 500_009);
    }

    #[test]
    fn cache_lines_errors_with_no_active_session() {
        let state = SessionState::default();
        let result = cache_lines(&state, 0, "unified".to_string(), 0, 10);
        assert_eq!(result.unwrap_err(), "no active simulation session");
    }

    #[test]
    fn cache_lines_rejects_a_count_over_the_max() {
        let state = SessionState::default();
        let result = cache_lines(&state, 0, "unified".to_string(), 0, 513);
        assert!(result.unwrap_err().contains("exceeds the maximum"));
    }

    fn oversized_associativity_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            page_size: 4096,
            hierarchy: vec![Level {
                id: "l1".into(),
                harvard: false,
                unified: Some(CacheEntry {
                    exp: 1,
                    config: CacheConfig {
                        block_size: 4,
                        associativity: 10_000,
                        replacement: ReplacementPolicy::Lru,
                        write_policy: WritePolicy::WriteBack,
                        write_allocate: true,
                        hit_time: 1,
                        dirty_percent: 0,
                    },
                }),
                i: None,
                d: None,
                inclusion: InclusionPolicy::Nine,
                bus_width: 32,
            }],
            tlb: TlbConfig { page_walk_start: "RAM".into(), entries: 4, associativity: 4, replacement: TlbReplacementPolicy::Lru, latency: 1 },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    #[test]
    fn cache_lines_bounds_a_single_oversized_set_even_though_cache_sets_could_not() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), oversized_associativity_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);

        let lines = cache_lines(&state, 0, "unified".to_string(), 0, 512).unwrap();
        assert_eq!(lines.len(), 512);
        assert!(lines.iter().all(|l| l.set_index == 0));
    }

    fn vm_hierarchy_config() -> HierarchyConfig {
        let mut cfg = empty_hierarchy_config();
        cfg.vm_enabled = true;
        cfg.page_size = 4;
        cfg.ram.frames = 4;
        cfg
    }

    #[test]
    fn get_tlb_info_errors_with_no_active_session() {
        let state = SessionState::default();
        assert_eq!(tlb_info(&state).unwrap_err(), "no active simulation session");
    }

    #[test]
    fn get_tlb_info_reports_configured_sizing() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        let info = tlb_info(&state).unwrap();
        assert_eq!(info.associativity, 4);
        assert_eq!(info.num_sets, 1);
    }

    #[test]
    fn get_tlb_sets_rejects_a_count_over_the_max() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        assert!(tlb_sets(&state, 0, 513).unwrap_err().contains("exceeds the maximum"));
    }

    #[test]
    fn get_frames_reports_configured_frame_content() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        let snaps = frame_snapshots(&state, 0, 4).unwrap();
        assert_eq!(snaps.len(), 4); // ram.frames is 4
        assert!(snaps.iter().all(|s| !s.valid));
    }

    #[test]
    fn frame_lines_errors_with_no_active_session() {
        let state = SessionState::default();
        assert_eq!(frame_lines(&state, 0, 4).unwrap_err(), "no active simulation session");
    }

    #[test]
    fn frame_lines_rejects_a_count_over_the_max() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        assert!(frame_lines(&state, 0, 513).unwrap_err().contains("exceeds the maximum"));
    }

    #[test]
    fn frame_lines_reports_one_row_per_frame_when_the_page_fits_in_8_words() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        let lines = frame_lines(&state, 0, 4).unwrap();
        assert_eq!(lines.len(), 4); // ram.frames is 4, page_size 4 fits in one row
        assert!(lines.iter().all(|l| !l.valid));
    }

    #[test]
    fn get_page_table_entries_starts_empty() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        let rows = page_table_entries(&state, 1, None, 0, 10).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn get_page_table_entries_rejects_a_count_over_the_max() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        assert!(page_table_entries(&state, 1, None, 0, 513).unwrap_err().contains("exceeds the maximum"));
    }

    #[test]
    fn get_page_table_level_bits_reports_the_configured_levels_widths() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        let guard = state.session.lock().unwrap();
        let bits = guard.as_ref().unwrap().memory().page_table_level_bits().to_vec();
        assert!(!bits.is_empty());
    }

    #[test]
    fn get_disk_pages_reports_the_configured_window() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), vm_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        let pages = disk_pages(&state, 0, 3).unwrap();
        assert_eq!(pages.len(), 3);
    }
}
