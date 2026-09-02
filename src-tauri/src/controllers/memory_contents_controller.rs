// Commands used by the memory-contents inspection panels (the registers view,
// cache level view, and RAM view) to read the current simulation's registers,
// cache snapshots, and RAM blocks. Session setup and the live event stream
// live in a separate file, simulation_session_controller.rs.
use serde::Serialize;

use crate::models::simulation::memory::hierarchy::{CacheLevelInfo, CacheSetSnapshot, CacheSide, RamBlockSnapshot};
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
    // Limits how many sets one call can return (the same idea as the RAM
    // block limit below). This does not limit how many sets a cache level
    // can have in total, since that number can be huge; the frontend fetches
    // it in pages instead of requesting everything at once.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::Base;
    use crate::models::simulation::memory::config::{
        CacheConfig, CacheEntry, DiskConfig, HierarchyConfig, InclusionPolicy, Level, RamConfig, ReplacementPolicy, TlbConfig,
        TlbReplacementPolicy, WritePolicy,
    };
    use crate::controllers::simulation_session_controller::build_session;

    fn empty_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            hierarchy: vec![],
            tlb: TlbConfig { page_size: 4096, page_table_levels: 1, page_walk_start: "RAM".into(), entries: 4, associativity: 4, replacement: TlbReplacementPolicy::Lru },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
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
        // This works out to 1,048,576 sets, far more than a single call would
        // ever request, but that's fine since the whole level no longer needs
        // to fit into one response. Fetching the level's info separately from
        // fetching its sets in pages is exactly what avoids that problem.
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
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
            tlb: TlbConfig { page_size: 4096, page_table_levels: 1, page_walk_start: "RAM".into(), entries: 4, associativity: 4, replacement: TlbReplacementPolicy::Lru },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
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
}
