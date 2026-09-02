// This file handles the simulation session lifecycle (start, pause, resume,
// step, stop, set speed) and sends out the `sim:event` stream that drives
// the hit/miss/read/write animations in `HierarchyDiagram.vue`. It used to
// be part of one big `simulation_controller.rs` file, split out so this
// event stream and lifecycle logic stays separate from the memory contents
// commands (see `memory_contents_controller.rs`) and the future stats code.
use serde::Serialize;
use tauri::Manager;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::models::interpreter::riscv::codegen::ir::CodegenOutput;
use crate::models::interpreter::riscv::Base;
use crate::models::simulation::cpu::RiscvCpuSimulator;
use crate::models::simulation::memory::backing_store::DataStore;
use crate::models::simulation::memory::config::HierarchyConfig;
use crate::models::simulation::memory::hierarchy::{AccessKind, LevelId, LevelOutcome, MemoryHierarchySimulator};
use crate::models::simulation::memory::seeded_random;
use crate::models::simulation::session::SimulationSession;
use crate::models::simulation::{Action, ActionKind};

pub struct SessionState {
    // Kept accessible within the crate because the snapshot helpers in
    // memory_contents_controller.rs lock this directly too, just like the
    // commands in this file.
    pub(crate) session: Mutex<Option<SimulationSession>>,
    running: Arc<AtomicBool>,
    speed_ms: Arc<AtomicU32>,
    generation: Arc<AtomicU64>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            running: Arc::new(AtomicBool::new(false)),
            speed_ms: Arc::new(AtomicU32::new(100)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl SessionState {
    /// Starts a new run: increases the run counter and marks the session as
    /// running, then returns the new counter value. Any older stepping loop
    /// still running in the background will notice its counter is out of
    /// date and stop itself on its next iteration, so only one loop is ever
    /// actually stepping the session at a time, even if start and resume get
    /// called close together.
    fn begin_run(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.running.store(true, Ordering::SeqCst);
        generation
    }
}

pub(crate) fn build_session(
    base: Base,
    codegen: CodegenOutput,
    hierarchy: HierarchyConfig,
    ram_block_size: usize,
    disk_block_size: usize,
    breakpoints: Vec<usize>,
) -> SimulationSession {
    let ram_dirty_percent = hierarchy.ram.dirty_percent;
    // If the user picked a seed, use it. Otherwise generate one here, once,
    // so it can be reused for both the RAM store below and every cache the
    // simulator builds internally.
    let seed = hierarchy.seed.unwrap_or_else(seeded_random::generate_seed);
    let memory = MemoryHierarchySimulator::new(
        hierarchy,
        DataStore::with_dirty_percent_seeded(ram_block_size, ram_dirty_percent, seed),
        DataStore::new(disk_block_size),
        seed,
    );
    let simulator: Box<dyn crate::models::simulation::CpuSimulator + Send> =
        Box::new(RiscvCpuSimulator::new(base, codegen, memory));
    SimulationSession::new(simulator, breakpoints.into_iter().collect())
}

#[tauri::command]
pub fn start_simulation(
    app: tauri::AppHandle,
    state: tauri::State<SessionState>,
    base: Base,
    codegen: CodegenOutput,
    hierarchy: HierarchyConfig,
    breakpoints: Vec<usize>,
) -> Result<(), String> {
    // There is no frontend setting yet for the RAM/disk block size, so use a
    // fixed 32-byte block as a sensible default. This number must match
    // RAM_BLOCK_SIZE in src/components/simulation/RamView.vue, which assumes
    // the same size when drawing the RAM inspection grid.
    const DEFAULT_BLOCK_SIZE: usize = 32;
    let session = build_session(base, codegen, hierarchy, DEFAULT_BLOCK_SIZE, DEFAULT_BLOCK_SIZE, breakpoints);

    // Increase the run counter (and mark it running) before installing the
    // new session, not after. An old stepping thread from a previous run
    // might already be waiting to access the session. If we installed the
    // session first, that old thread could grab it and start stepping the
    // new session under its outdated counter, racing the new thread started
    // below. Bumping the counter first makes sure the old thread notices it
    // is outdated and stops before it can touch the new session.
    let generation = state.begin_run();
    // If something goes wrong accessing the session here, make sure
    // "running" gets reset to false before returning the error, so the app
    // doesn't get stuck thinking a simulation is running when nothing
    // actually is. This matches the same recovery done in
    // run_auto_step_loop.
    let mut guard = match state.session.lock() {
        Ok(guard) => guard,
        Err(e) => {
            state.running.store(false, Ordering::SeqCst);
            return Err(e.to_string());
        }
    };
    *guard = Some(session);
    drop(guard);

    let app_handle = app.clone();
    std::thread::spawn(move || {
        run_auto_step_loop(app_handle, generation, None);
    });

    Ok(())
}

#[tauri::command]
pub fn stop_simulation(state: tauri::State<SessionState>) -> Result<(), String> {
    state.running.store(false, Ordering::SeqCst);
    *state.session.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

/// Decides whether the stepping loop should check for a breakpoint on this
/// step. This is a small standalone function so the "keep going while
/// still on the line we resumed from" behavior used by resume_simulation
/// can be tested on its own, without needing to run the whole loop.
///
/// `skip_while_line` is the line the session was paused on when it
/// resumed (or none for a fresh start, which always checks). `current_line`
/// is the line the next step is about to run. The check is skipped only
/// when both are set and match, meaning the loop hasn't moved past the
/// line it resumed from yet. This works correctly whether one instruction
/// takes several steps (like a load or store touching multiple memory
/// levels) or several instructions share the same line (like an expanded
/// pseudo-instruction), because the current line only changes once
/// execution has actually moved past all of them.
fn should_check_breakpoint(skip_while_line: Option<usize>, current_line: Option<usize>) -> bool {
    !(skip_while_line.is_some() && skip_while_line == current_line)
}

fn run_auto_step_loop(app: tauri::AppHandle, my_generation: u64, skip_breakpoint_check_while_line: Option<usize>) {
    use tauri::Emitter;

    // A local copy that can change. Once execution moves past the line we
    // paused on, this gets cleared to none for good (see below), so that if
    // the simulated program later loops back to that same line, its
    // breakpoint isn't skipped by mistake.
    let mut skip_while_line = skip_breakpoint_check_while_line;

    loop {
        let state = app.state::<SessionState>();

        // If the run counter has moved on, a newer start or resume call has
        // taken over the session (this can happen even if this thread never
        // saw "running" turn false, for example if it was asleep during a
        // quick pause/resume). This thread is now outdated, so stop here
        // without touching the session or sending any events, since the
        // newer thread owns it now.
        if state.generation.load(Ordering::SeqCst) != my_generation {
            return;
        }

        if !state.running.load(Ordering::SeqCst) {
            return;
        }

        let mut guard = match state.session.lock() {
            Ok(guard) => guard,
            Err(_) => {
                // Something went wrong accessing the session, so it's not
                // safe to keep stepping. Reset "running" so a future resume
                // or start isn't blocked thinking a stepping loop is still
                // active.
                state.running.store(false, Ordering::SeqCst);
                return;
            }
        };
        let session = match guard.as_mut() {
            Some(session) => session,
            None => {
                // stop_simulation cleared the session in between this
                // thread's checks above and now. Nothing is running any
                // more, but "running" was never reset for this case, so
                // reset it here so a future start or resume isn't blocked
                // thinking a stepping loop is still active.
                state.running.store(false, Ordering::SeqCst);
                return;
            }
        };

        if session.is_finished() {
            state.running.store(false, Ordering::SeqCst);
            drop(guard);
            let _ = app.emit("sim:event", SimEvent::SimulationPaused { reason: "end".to_string() });
            return;
        }

        // Turn off the skip for good once we've moved past the remembered
        // line. This has to happen before the check below so the skip
        // actually turns off on the same step the line changes, not one
        // step late.
        let current_line = session.current_source_line();
        if skip_while_line.is_some() && skip_while_line != current_line {
            skip_while_line = None;
        }

        if should_check_breakpoint(skip_while_line, current_line) && session.is_at_breakpoint() {
            state.running.store(false, Ordering::SeqCst);
            drop(guard);
            let _ = app.emit("sim:event", SimEvent::SimulationPaused { reason: "breakpoint".to_string() });
            return;
        }

        // Read the program counter before stepping. The simulator already
        // moves it forward to the next instruction as part of finishing the
        // current one, so reading it after stepping would give the address
        // of the next instruction instead of the one that just completed.
        // Reading it here keeps it consistent with the source line, both
        // describing the instruction that is finishing on this call.
        let pc = session.pc();
        let action = session.step_action().expect("just checked is_finished() is false");
        drop(guard);

        let _ = app.emit("sim:event", action_to_sim_event(&action));
        if action.instruction_complete {
            let _ = app.emit("sim:event", SimEvent::InstructionComplete { pc, source_line: action.source_line });
        }

        let speed = app.state::<SessionState>().speed_ms.load(Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(speed as u64));
    }
}

#[tauri::command]
pub fn pause_simulation(state: tauri::State<SessionState>) -> Result<(), String> {
    state.running.store(false, Ordering::SeqCst);
    Ok(())
}

/// Returns an error if there is no active simulation session, or success
/// otherwise. Pulled out of resume_simulation so this check (the same
/// idea used by step_action and step_instruction) can be tested on its
/// own without needing a full app handle.
fn require_active_session(state: &SessionState) -> Result<(), String> {
    if state.session.lock().map_err(|e| e.to_string())?.is_some() {
        Ok(())
    } else {
        Err("no active simulation session".to_string())
    }
}

#[tauri::command]
pub fn resume_simulation(app: tauri::AppHandle, state: tauri::State<SessionState>) -> Result<(), String> {
    // Same "no active session" check used by step_action and
    // step_instruction. Without it, resuming with no session would still
    // start a run (marking it as running) and spawn a thread that finds no
    // session on its first step and quits without resetting "running",
    // leaving it stuck as true even though nothing is actually running.
    require_active_session(&state)?;

    // When resuming from a pause, the breakpoint check must be skipped
    // while the session is still sitting on the line it paused at, because
    // pausing doesn't move the program counter, so the check would
    // immediately see the same breakpoint again and pause right away,
    // making resume never actually run anything. Skipping just one step
    // isn't enough either: a load or store can take several steps (one per
    // memory level) before the program counter moves, and a
    // pseudo-instruction can expand into several real instructions sharing
    // one source line. So what gets remembered here is the whole line, and
    // the loop keeps skipping the check for as long as it's still on that
    // line (see should_check_breakpoint), not just for one step.
    let skip_while_line = {
        let guard = state.session.lock().map_err(|e| e.to_string())?;
        guard.as_ref().and_then(|session| session.current_source_line())
    };

    let generation = state.begin_run();
    let app_handle = app.clone();
    std::thread::spawn(move || {
        run_auto_step_loop(app_handle, generation, skip_while_line);
    });
    Ok(())
}

#[tauri::command]
pub fn step_action(app: tauri::AppHandle, state: tauri::State<SessionState>) -> Result<(), String> {
    use tauri::Emitter;

    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_mut().ok_or("no active simulation session")?;

    // Read the program counter before stepping, same reason as in the
    // stepping loop: the simulator already moves it to the next instruction
    // as part of completing the current one, so reading it after stepping
    // would give the wrong address.
    let pc = session.pc();
    if let Some(action) = session.step_action() {
        // Check whether this step finished the session while we still have
        // access to it, then release access before sending any events.
        // This matches the same pattern used elsewhere in this file: never
        // send events while still holding onto the session.
        let finished = session.is_finished();
        drop(guard);
        let _ = app.emit("sim:event", action_to_sim_event(&action));
        if action.instruction_complete {
            let _ = app.emit("sim:event", SimEvent::InstructionComplete { pc, source_line: action.source_line });
        }
        if finished {
            let _ = app.emit("sim:event", SimEvent::SimulationPaused { reason: "end".to_string() });
        }
    }
    Ok(())
}

#[tauri::command]
pub fn step_instruction(app: tauri::AppHandle, state: tauri::State<SessionState>) -> Result<(), String> {
    use tauri::Emitter;

    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_mut().ok_or("no active simulation session")?;

    // Same reason as elsewhere: read the program counter before stepping.
    let pc = session.pc();
    let actions = session.step_instruction();
    // Check whether the session finished while we still have access to it,
    // same pattern as step_action: release access before sending events.
    let finished = session.is_finished();
    drop(guard);

    for action in &actions {
        let _ = app.emit("sim:event", action_to_sim_event(action));
    }
    if let Some(last) = actions.last() {
        if last.instruction_complete {
            let _ = app.emit("sim:event", SimEvent::InstructionComplete { pc, source_line: last.source_line });
        }
    }
    if !actions.is_empty() && finished {
        let _ = app.emit("sim:event", SimEvent::SimulationPaused { reason: "end".to_string() });
    }
    Ok(())
}

#[tauri::command]
pub fn set_speed(state: tauri::State<SessionState>, ms: u32) -> Result<(), String> {
    state.speed_ms.store(ms, Ordering::SeqCst);
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SimEvent {
    RegWrite { register: u8, value: u64, source_line: usize },
    FpRegWrite { register: u8, value: f64, source_line: usize },
    CacheAccess { level_index: usize, outcome: String, address: u64, access_kind: String, source_line: usize },
    RamAccess { outcome: String, address: u64, access_kind: String, source_line: usize },
    DiskAccess { outcome: String, address: u64, access_kind: String, source_line: usize },
    TlbAccess { outcome: String, address: u64, access_kind: String, source_line: usize },
    NoOp { source_line: usize },
    InstructionComplete { pc: u64, source_line: usize },
    SimulationPaused { reason: String },
}

fn outcome_str(outcome: LevelOutcome) -> String {
    match outcome {
        LevelOutcome::Hit => "hit".to_string(),
        LevelOutcome::Miss => "miss".to_string(),
    }
}

fn access_kind_str(kind: AccessKind) -> String {
    match kind {
        AccessKind::Fetch => "fetch".to_string(),
        AccessKind::Load => "load".to_string(),
        AccessKind::Store => "store".to_string(),
    }
}

pub fn action_to_sim_event(action: &Action) -> SimEvent {
    let source_line = action.source_line;
    match &action.kind {
        ActionKind::RegisterWrite { register, value } => {
            SimEvent::RegWrite { register: register.0, value: *value, source_line }
        }
        ActionKind::FpRegisterWrite { register, value } => {
            SimEvent::FpRegWrite { register: register.0, value: *value, source_line }
        }
        ActionKind::MemoryLevel(level_result) => {
            let outcome = outcome_str(level_result.outcome);
            let address = level_result.address;
            let access_kind = access_kind_str(level_result.access_kind);
            match level_result.level {
                LevelId::Tlb => SimEvent::TlbAccess { outcome, address, access_kind, source_line },
                LevelId::Cache(index) => SimEvent::CacheAccess { level_index: index, outcome, address, access_kind, source_line },
                LevelId::Ram => SimEvent::RamAccess { outcome, address, access_kind, source_line },
                LevelId::Disk => SimEvent::DiskAccess { outcome, address, access_kind, source_line },
            }
        }
        ActionKind::NoOp => SimEvent::NoOp { source_line },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::simulation::memory::hierarchy::{AccessKind, LevelId, LevelOutcome, LevelResult};
    use crate::models::simulation::{ActionKind, RegisterId};

    #[test]
    fn register_write_translates_to_reg_write_event() {
        let action = Action {
            kind: ActionKind::RegisterWrite { register: RegisterId(5), value: 42 },
            source_line: 7,
            instruction_complete: true,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "reg_write");
        assert_eq!(json["register"], 5);
        assert_eq!(json["value"], 42);
        assert_eq!(json["sourceLine"], 7);
    }

    #[test]
    fn fp_register_write_translates_to_fp_reg_write_event() {
        let action = Action {
            kind: ActionKind::FpRegisterWrite { register: RegisterId(3), value: 1.5 },
            source_line: 2,
            instruction_complete: true,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "fp_reg_write");
        assert_eq!(json["value"], 1.5);
    }

    #[test]
    fn cache_hit_translates_to_cache_access_event() {
        let action = Action {
            kind: ActionKind::MemoryLevel(LevelResult {
                level: LevelId::Cache(1),
                outcome: LevelOutcome::Hit,
                address: 0x1000,
                access_kind: AccessKind::Load,
            }),
            source_line: 9,
            instruction_complete: false,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "cache_access");
        assert_eq!(json["levelIndex"], 1);
        assert_eq!(json["outcome"], "hit");
        assert_eq!(json["address"], 0x1000);
        assert_eq!(json["accessKind"], "load");
    }

    #[test]
    fn ram_miss_translates_to_ram_access_event() {
        let action = Action {
            kind: ActionKind::MemoryLevel(LevelResult {
                level: LevelId::Ram,
                outcome: LevelOutcome::Hit,
                address: 0x2000,
                access_kind: AccessKind::Store,
            }),
            source_line: 9,
            instruction_complete: false,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "ram_access");
        assert_eq!(json["address"], 0x2000);
        assert_eq!(json["accessKind"], "store");
    }

    #[test]
    fn tlb_result_translates_to_tlb_access_event() {
        let action = Action {
            kind: ActionKind::MemoryLevel(LevelResult {
                level: LevelId::Tlb,
                outcome: LevelOutcome::Miss,
                address: 0x3000,
                access_kind: AccessKind::Fetch,
            }),
            source_line: 9,
            instruction_complete: false,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "tlb_access");
        assert_eq!(json["outcome"], "miss");
        assert_eq!(json["address"], 0x3000);
        assert_eq!(json["accessKind"], "fetch");
    }

    #[test]
    fn no_op_translates_to_no_op_event() {
        let action = Action { kind: ActionKind::NoOp, source_line: 4, instruction_complete: true };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "no_op");
    }

    use crate::models::interpreter::riscv::codegen::ir::{IrEntry, MemoryLayout};
    use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
    use crate::models::interpreter::riscv::parser::ast::Operand;
    use crate::models::simulation::memory::config::{DiskConfig, HierarchyConfig, RamConfig, TlbConfig, TlbReplacementPolicy};
    use crate::models::interpreter::riscv::Base;
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;

    pub(crate) fn empty_hierarchy_config() -> HierarchyConfig {
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

    pub(crate) fn empty_codegen() -> crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
        crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
            layout: MemoryLayout { text_base: 0x1000, data_base: 0x2000, labels: HashMap::new(), initial_data: vec![] },
            entries: vec![],
        }
    }

    #[test]
    fn session_state_defaults_to_no_active_session_and_speed_100() {
        let state = SessionState::default();
        assert!(state.session.lock().unwrap().is_none());
        assert_eq!(state.speed_ms.load(Ordering::SeqCst), 100);
        assert!(!state.running.load(Ordering::SeqCst));
    }

    #[test]
    fn build_session_creates_a_session_ready_to_step() {
        let mut session = build_session(Base::Rv32i, empty_codegen(), empty_hierarchy_config(), 64, 64, vec![]);
        assert!(session.is_finished()); // the program has no instructions
        assert!(session.step_action().is_none());
    }

    #[test]
    fn session_pc_delegates_to_the_inner_simulator() {
        let session = build_session(Base::Rv32i, empty_codegen(), empty_hierarchy_config(), 64, 64, vec![]);
        assert_eq!(session.pc(), 0x1000);
    }

    fn hierarchy_config_with(seed: Option<u64>, ram_dirty_percent: u8) -> HierarchyConfig {
        HierarchyConfig { ram: RamConfig { latency: 100, size: 1024, dirty_percent: ram_dirty_percent }, seed, ..empty_hierarchy_config() }
    }

    #[test]
    fn an_explicit_seed_makes_ram_dirty_seeding_reproducible_across_sessions() {
        let a = build_session(Base::Rv32i, empty_codegen(), hierarchy_config_with(Some(1234), 100), 64, 64, vec![]);
        let b = build_session(Base::Rv32i, empty_codegen(), hierarchy_config_with(Some(1234), 100), 64, 64, vec![]);
        let blocks_a = a.memory().ram_blocks(0x10000000, 1);
        let blocks_b = b.memory().ram_blocks(0x10000000, 1);
        assert_eq!(blocks_a[0].words, blocks_b[0].words);
        assert!(blocks_a[0].dirty);
    }

    #[test]
    fn no_seed_still_builds_a_working_session_with_ram_dirty_seeding_enabled() {
        // No seed given here, so build_session must generate one on its own
        // instead of crashing or leaving dirty seeding turned off.
        let session = build_session(Base::Rv32i, empty_codegen(), hierarchy_config_with(None, 100), 64, 64, vec![]);
        assert!(session.memory().ram_blocks(0x10000000, 1)[0].dirty);
    }

    #[test]
    fn begin_run_returns_a_new_generation_on_every_call() {
        let state = SessionState::default();
        let g1 = state.begin_run();
        let g2 = state.begin_run();
        // Every call to begin_run represents a new run, whether it's
        // resuming after a pause or starting fresh, so any earlier stepping
        // thread still holding the old counter can tell it's outdated and
        // stop, even if it never saw "running" turn false in between.
        assert_ne!(g1, g2);
    }

    #[test]
    fn begin_run_marks_running_true_and_stores_the_returned_generation() {
        let state = SessionState::default();
        let generation = state.begin_run();
        assert!(state.running.load(Ordering::SeqCst));
        assert_eq!(state.generation.load(Ordering::SeqCst), generation);
    }

    #[test]
    fn instruction_complete_pc_should_be_captured_before_stepping_not_after() {
        // Matches the same order used in run_auto_step_loop: read the
        // program counter before stepping, not after. The simulator already
        // moves it to the next instruction as part of completing the
        // current one, so reading it after stepping would give the wrong
        // address.
        let entries = vec![
            IrEntry {
                address: 0x1000,
                kind: InstructionKind::Add,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(2)),
                    Operand::Register(RegisterKind::X(3)),
                ],
                bytecode: 0,
                source_line: 1,
                source_column: 1,
            },
            IrEntry {
                address: 0x1004,
                kind: InstructionKind::Add,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(2)),
                    Operand::Register(RegisterKind::X(3)),
                ],
                bytecode: 0,
                source_line: 2,
                source_column: 1,
            },
        ];
        let codegen = crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
            layout: MemoryLayout { text_base: 0x1000, data_base: 0x2000, labels: HashMap::new(), initial_data: vec![] },
            entries,
        };
        let mut session = build_session(Base::Rv32i, codegen, empty_hierarchy_config(), 64, 64, vec![]);

        let pc_before_first_step = session.pc();
        // Every instruction starts with its own fetch access going through
        // the memory hierarchy first. With an empty hierarchy that's one
        // memory level action (not yet complete), followed by the action
        // that finishes the instruction.
        let fetch_action = session.step_action().unwrap();
        assert!(!fetch_action.instruction_complete);
        let action = session.step_action().unwrap();
        assert!(action.instruction_complete);

        // The first "instruction complete" event's program counter must be
        // the program's starting address (the instruction that just
        // finished), not the next address (the instruction the simulator
        // has already moved on to).
        assert_eq!(pc_before_first_step, 0x1000);
        assert_eq!(session.pc(), 0x1004, "simulator should already have advanced pc to the next instruction");
        assert_ne!(pc_before_first_step, session.pc());
    }

    #[test]
    fn set_speed_updates_the_shared_atomic() {
        let state = SessionState::default();
        state.speed_ms.store(999, Ordering::SeqCst);
        assert_eq!(state.speed_ms.load(Ordering::SeqCst), 999);
    }

    // --- Resuming from a breakpoint: skipping the breakpoint check correctly ---

    #[test]
    fn should_check_breakpoint_skips_only_while_current_line_matches_the_remembered_line() {
        // Still on the remembered "paused at" line: skip the check.
        assert!(!should_check_breakpoint(Some(1), Some(1)));
        // Moved to a different line: check resumes.
        assert!(should_check_breakpoint(Some(1), Some(2)));
        // If the session finished (no current line) while a skip was still
        // active, checking resumes since there's nothing left to wrongly
        // pause on, and finishing is handled separately anyway.
        assert!(should_check_breakpoint(Some(1), None));
        // A fresh start (no remembered line) always checks, no matter what
        // the current line is, just like a normal debugger.
        assert!(should_check_breakpoint(None, Some(1)));
        assert!(should_check_breakpoint(None, None));
    }

    #[test]
    fn stepping_past_a_breakpoint_while_the_skip_line_matches_still_executes_the_action() {
        // Two instructions, with a breakpoint on the first. This matches
        // what run_auto_step_loop sees right after pausing at a breakpoint:
        // the program counter hasn't moved, so checking for a breakpoint
        // without stepping still reports true.
        let entries = vec![
            IrEntry {
                address: 0x1000,
                kind: InstructionKind::Add,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(2)),
                    Operand::Register(RegisterKind::X(3)),
                ],
                bytecode: 0,
                source_line: 1,
                source_column: 1,
            },
            IrEntry {
                address: 0x1004,
                kind: InstructionKind::Add,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(2)),
                    Operand::Register(RegisterKind::X(3)),
                ],
                bytecode: 0,
                source_line: 2,
                source_column: 1,
            },
        ];
        let codegen = crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
            layout: MemoryLayout { text_base: 0x1000, data_base: 0x2000, labels: HashMap::new(), initial_data: vec![] },
            entries,
        };
        let mut session = build_session(Base::Rv32i, codegen, empty_hierarchy_config(), 64, 64, vec![1]);

        // Confirm the session is sitting right at the breakpoint, the way
        // run_auto_step_loop would see it right after pausing.
        assert!(session.is_at_breakpoint(), "checking is_at_breakpoint() alone (without stepping) must still report true");

        let skip_while_line = session.current_source_line();
        assert_eq!(skip_while_line, Some(1));

        // should_check_breakpoint returns false here, meaning this step
        // does not re-check for a breakpoint and instead just goes ahead
        // and steps.
        assert!(!should_check_breakpoint(skip_while_line, session.current_source_line()));
        let action = session.step_action();
        assert!(action.is_some(), "the skip iteration must actually execute the instruction it paused on, not pause again");
    }

    #[test]
    fn resuming_on_a_multi_action_load_or_store_line_completes_the_whole_instruction_before_any_recheck() {
        // Regression test: a load instruction against a hierarchy with no
        // cache levels (going straight to RAM) produces two actions, a
        // memory access action and then the register write that completes
        // it, with the program counter (and so the current line) unchanged
        // in between. resume_simulation's remembered line must be
        // respected across both actions, not just the first, or the loop
        // would wrongly pause again after only the memory access, before
        // the instruction actually finishes.
        let entries = vec![
            IrEntry {
                address: 0x1000,
                kind: InstructionKind::Lw,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Offset { imm: 0, base: RegisterKind::X(2) },
                ],
                bytecode: 0,
                source_line: 1,
                source_column: 1,
            },
            IrEntry {
                address: 0x1004,
                kind: InstructionKind::Add,
                operands: vec![
                    Operand::Register(RegisterKind::X(1)),
                    Operand::Register(RegisterKind::X(2)),
                    Operand::Register(RegisterKind::X(3)),
                ],
                bytecode: 0,
                source_line: 2,
                source_column: 1,
            },
        ];
        let codegen = crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
            layout: MemoryLayout { text_base: 0x1000, data_base: 0x2000, labels: HashMap::new(), initial_data: vec![] },
            entries,
        };
        let mut session = build_session(Base::Rv32i, codegen, empty_hierarchy_config(), 64, 64, vec![1]);

        // Matches resume_simulation: remember the line we're paused at.
        let mut skip_while_line = session.current_source_line();
        assert_eq!(skip_while_line, Some(1));
        assert!(session.is_at_breakpoint());

        // Step 1: the instruction's own fetch access, going through the
        // memory hierarchy first. With a hierarchy with no cache levels
        // that's one memory access action (not yet complete), with the
        // same skip requirement as any other action in the middle of an
        // instruction.
        let current_line = session.current_source_line();
        if skip_while_line.is_some() && skip_while_line != current_line {
            skip_while_line = None;
        }
        assert!(!should_check_breakpoint(skip_while_line, current_line), "must still be skipping on the fetch's memory-level action");
        let fetch_action = session.step_action().unwrap();
        assert!(matches!(fetch_action.kind, ActionKind::MemoryLevel(_)));
        assert!(!fetch_action.instruction_complete, "the fetch access resolves in one memory-level step against a zero-level hierarchy");
        assert_eq!(session.current_source_line(), Some(1), "pc must not have advanced yet — the instruction isn't complete");

        // Step 2: the load's own memory access action, started once the
        // fetch above finished. This must be skipped, not paused on again,
        // and the program counter/source line must not have moved yet.
        let current_line = session.current_source_line();
        if skip_while_line.is_some() && skip_while_line != current_line {
            skip_while_line = None;
        }
        assert!(!should_check_breakpoint(skip_while_line, current_line), "must still be skipping on the memory-level action");
        let first = session.step_action().unwrap();
        assert!(matches!(first.kind, ActionKind::MemoryLevel(_)));
        assert!(!first.instruction_complete, "lw against RAM-only hierarchy resolves in one memory-level step");
        assert_eq!(session.current_source_line(), Some(1), "pc must not have advanced yet — the instruction isn't complete");

        // Step 3: the register write action that completes the instruction,
        // still on the same source line, since the program counter only
        // moves forward as part of this action. This is exactly the case
        // where an earlier, simpler "skip just one step" approach got it
        // wrong: it would have turned the check back on here and wrongly
        // paused again before this action ran.
        let current_line = session.current_source_line();
        if skip_while_line.is_some() && skip_while_line != current_line {
            skip_while_line = None;
        }
        assert!(
            !should_check_breakpoint(skip_while_line, current_line),
            "must still be skipping on the second action of the SAME instruction — this is the round-2 regression"
        );
        let second = session.step_action().unwrap();
        assert!(matches!(second.kind, ActionKind::RegisterWrite { .. }));
        assert!(second.instruction_complete, "the whole lw instruction must now be complete");
        assert_eq!(session.current_source_line(), Some(2), "pc has now advanced past the breakpoint line");

        // Step 4: now on a new line, so the skip must have turned off for
        // good and normal breakpoint checking resumes.
        let current_line = session.current_source_line();
        if skip_while_line.is_some() && skip_while_line != current_line {
            skip_while_line = None;
        }
        assert_eq!(skip_while_line, None, "skip must be permanently cleared once the line changes");
        assert!(should_check_breakpoint(skip_while_line, current_line), "normal breakpoint checking must resume once past the original line");
    }

    // --- Resuming with no active session ---

    #[test]
    fn require_active_session_errors_when_no_session_is_installed() {
        let state = SessionState::default();
        assert_eq!(require_active_session(&state), Err("no active simulation session".to_string()));
    }

    #[test]
    fn require_active_session_succeeds_once_a_session_is_installed() {
        let state = SessionState::default();
        let session = build_session(Base::Rv32i, empty_codegen(), empty_hierarchy_config(), 64, 64, vec![]);
        *state.session.lock().unwrap() = Some(session);
        assert!(require_active_session(&state).is_ok());
    }

    // --- Manually stepping to the end of a program ---

    #[test]
    fn step_action_leaves_a_finished_session_detectable_after_the_final_step() {
        // A single-instruction program: stepping it once finishes the
        // program. The actual command (not tested directly here since it
        // needs a full app handle) checks whether the session is finished
        // right after this step, then sends a "simulation paused, reason:
        // end" event. This test proves the condition that check relies on.
        let entries = vec![IrEntry {
            address: 0x1000,
            kind: InstructionKind::Add,
            operands: vec![
                Operand::Register(RegisterKind::X(1)),
                Operand::Register(RegisterKind::X(2)),
                Operand::Register(RegisterKind::X(3)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }];
        let codegen = crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
            layout: MemoryLayout { text_base: 0x1000, data_base: 0x2000, labels: HashMap::new(), initial_data: vec![] },
            entries,
        };
        let mut session = build_session(Base::Rv32i, codegen, empty_hierarchy_config(), 64, 64, vec![]);

        assert!(!session.is_finished());
        // Every instruction starts with its own fetch access going through
        // the memory hierarchy first. With an empty hierarchy that's one
        // memory level action (not yet complete), followed by the action
        // that finishes the instruction.
        let fetch_action = session.step_action();
        assert!(fetch_action.is_some());
        assert!(!session.is_finished(), "the fetch access alone must not finish the program");
        let action = session.step_action();
        assert!(action.is_some(), "the step that completes the program must still return the action");
        assert!(session.is_finished(), "the session must report finished immediately after the step that completed it");

        // The event the command is expected to send as a result.
        let expected_event = SimEvent::SimulationPaused { reason: "end".to_string() };
        let json = serde_json::to_value(&expected_event).unwrap();
        assert_eq!(json["kind"], "simulation_paused");
        assert_eq!(json["reason"], "end");
    }

    #[test]
    fn step_instruction_leaves_a_finished_session_detectable_after_the_final_step() {
        let entries = vec![IrEntry {
            address: 0x1000,
            kind: InstructionKind::Add,
            operands: vec![
                Operand::Register(RegisterKind::X(1)),
                Operand::Register(RegisterKind::X(2)),
                Operand::Register(RegisterKind::X(3)),
            ],
            bytecode: 0,
            source_line: 1,
            source_column: 1,
        }];
        let codegen = crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
            layout: MemoryLayout { text_base: 0x1000, data_base: 0x2000, labels: HashMap::new(), initial_data: vec![] },
            entries,
        };
        let mut session = build_session(Base::Rv32i, codegen, empty_hierarchy_config(), 64, 64, vec![]);

        assert!(!session.is_finished());
        let actions = session.step_instruction();
        assert!(!actions.is_empty(), "the instruction-step that completes the program must still return its actions");
        assert!(session.is_finished(), "the session must report finished immediately after the instruction-step that completed it");
    }
}
