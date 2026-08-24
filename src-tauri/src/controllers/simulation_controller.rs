use serde::Serialize;
use tauri::Manager;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::models::interpreter::riscv::codegen::ir::CodegenOutput;
use crate::models::interpreter::riscv::Base;
use crate::models::simulation::cpu::RiscvCpuSimulator;
use crate::models::simulation::memory::backing_store::DataStore;
use crate::models::simulation::memory::config::HierarchyConfig;
use crate::models::simulation::memory::hierarchy::{LevelId, LevelOutcome, MemoryHierarchySimulator};
use crate::models::simulation::session::SimulationSession;
use crate::models::simulation::{Action, ActionKind};

pub struct SessionState {
    session: Mutex<Option<SimulationSession>>,
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
    /// Atomically starts a new "run": bumps the generation counter and marks
    /// `running` true, returning the new generation value. Any previously
    /// spawned `run_auto_step_loop` thread compares its own (now stale)
    /// generation against `self.generation` and exits on its next iteration,
    /// so at most one auto-run loop is ever actually stepping the session at
    /// a time, no matter how `start_simulation`/`resume_simulation` race.
    fn begin_run(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.running.store(true, Ordering::SeqCst);
        generation
    }
}

fn build_session(
    base: Base,
    codegen: CodegenOutput,
    hierarchy: HierarchyConfig,
    ram_block_size: usize,
    disk_block_size: usize,
    breakpoints: Vec<usize>,
) -> SimulationSession {
    let memory = MemoryHierarchySimulator::new(hierarchy, DataStore::new(ram_block_size), DataStore::new(disk_block_size));
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
    // Block size for the RAM/disk backing stores: this branch doesn't yet
    // plumb a dedicated frontend field for it, so a fixed 64-byte block is
    // used as a reasonable default (matches the most common cache block
    // size used throughout this project's tests) — revisit if a future
    // branch needs this configurable.
    const DEFAULT_BLOCK_SIZE: usize = 64;
    let session = build_session(base, codegen, hierarchy, DEFAULT_BLOCK_SIZE, DEFAULT_BLOCK_SIZE, breakpoints);

    // Bump the generation (and mark `running`) BEFORE installing the new
    // session, not after: a stale auto-run thread from a previous run may
    // already be blocked waiting on `state.session.lock()`. If we installed
    // the session first, releasing the lock would let that stale thread
    // acquire it and step the brand-new session under its OLD generation,
    // racing the freshly-spawned thread below. Bumping the generation first
    // means the stale thread's generation check fails before it can ever
    // reach the lock holding the new session.
    let generation = state.begin_run();
    // A poisoned lock here must not leave `running` stuck at `true` with no
    // session installed and no thread running: reset it before returning
    // the error, mirroring the poisoned-lock recovery already done inside
    // `run_auto_step_loop`.
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

/// Decides whether `run_auto_step_loop` should check `is_at_breakpoint()`
/// on the current iteration. Extracted as a small pure function so the
/// "skip while still on the paused line" semantics used by
/// `resume_simulation` are unit-testable without needing an `AppHandle` to
/// exercise the full loop.
///
/// `skip_while_line` is the source line the session was paused at when
/// resumed (or `None` for a fresh `start_simulation`, which always
/// checks). `current_line` is what `session.current_source_line()` reports
/// for the iteration about to run. The check is skipped only while both
/// are `Some` and equal — i.e. the loop hasn't yet moved past the line it
/// resumed from. This covers any number of atomic `Action`s within one
/// instruction (loads/stores touching multiple memory levels) and any
/// number of instructions sharing a source line (pseudo-instruction
/// expansions like `li`), uniformly: `current_line` only changes once `pc`
/// actually advances past the whole instruction (or run of instructions)
/// that shares that line.
fn should_check_breakpoint(skip_while_line: Option<usize>, current_line: Option<usize>) -> bool {
    !(skip_while_line.is_some() && skip_while_line == current_line)
}

fn run_auto_step_loop(app: tauri::AppHandle, my_generation: u64, skip_breakpoint_check_while_line: Option<usize>) {
    use tauri::Emitter;

    // Local, mutable copy: once the current source line moves past the
    // remembered "paused at" line, this is permanently cleared to `None`
    // for the rest of this thread's life (see the comment below), so a
    // later real loop in the user's program that revisits the same source
    // line does not spuriously re-skip its breakpoint.
    let mut skip_while_line = skip_breakpoint_check_while_line;

    loop {
        let state = app.state::<SessionState>();

        // A newer generation means a fresher `start_simulation`/`resume_simulation`
        // call has since taken ownership of this session (this can happen even
        // if this thread never observed `running` flip to false, e.g. it was
        // asleep during a pause/resume double-click). This thread is stale:
        // exit without touching the session at all — no lock, no emit — since
        // the newer thread is the sole owner now.
        if state.generation.load(Ordering::SeqCst) != my_generation {
            return;
        }

        if !state.running.load(Ordering::SeqCst) {
            return;
        }

        let mut guard = match state.session.lock() {
            Ok(guard) => guard,
            Err(_) => {
                // The lock is poisoned; there's no way to safely keep stepping.
                // Reset `running` so a future resume/start isn't permanently
                // locked out believing an auto-run loop is still alive.
                state.running.store(false, Ordering::SeqCst);
                return;
            }
        };
        let session = match guard.as_mut() {
            Some(session) => session,
            None => {
                // `stop_simulation` cleared the session between this
                // thread's generation/running checks and its lock
                // acquisition above. Nothing is running any more, but
                // `running` was never reset for this path — do so here so
                // a future start/resume isn't blocked believing an
                // auto-run loop is still alive.
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

        // Re-arm "skip" permanently once we've moved past the remembered
        // line: this must happen before computing `should_check_breakpoint`
        // below so the un-arm takes effect on the very iteration the line
        // changes, not one iteration later.
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

        // Capture `pc` BEFORE stepping: the underlying simulator already
        // advances its internal pc to the NEXT instruction as part of
        // completing the current one, so reading it after `step_action()`
        // would give the address of the instruction that's about to run
        // next rather than the one that just completed. Reading it here,
        // alongside the other pre-step state and still under the guard,
        // keeps `pc` consistent with `source_line`'s existing meaning: both
        // describe the instruction that's completing on this call.
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

/// Returns `Err("no active simulation session")` if no session is
/// currently installed, `Ok(())` otherwise. Extracted from
/// `resume_simulation` so the guard condition (mirroring the
/// `guard.as_mut().ok_or(...)` pattern `step_action`/`step_instruction`
/// already use) is unit-testable without an `AppHandle`.
fn require_active_session(state: &SessionState) -> Result<(), String> {
    if state.session.lock().map_err(|e| e.to_string())?.is_some() {
        Ok(())
    } else {
        Err("no active simulation session".to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct RegisterSnapshot {
    pub int: Vec<u64>,
    pub fp: Vec<f64>,
}

fn snapshot_registers(state: &SessionState) -> Result<RegisterSnapshot, String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = guard.as_ref().ok_or("no active simulation session")?;
    let int = (0..32u8).map(|i| session.register(crate::models::simulation::RegisterId(i))).collect();
    let fp = (0..32u8).map(|i| session.fp_register(crate::models::simulation::RegisterId(i))).collect();
    Ok(RegisterSnapshot { int, fp })
}

#[tauri::command]
pub fn resume_simulation(app: tauri::AppHandle, state: tauri::State<SessionState>) -> Result<(), String> {
    // Same "no active session" guard as `step_action`/`step_instruction`:
    // without it, resuming with no session would still call `begin_run()`
    // (setting `running = true`) and spawn a thread whose first iteration
    // finds no session and returns without resetting `running`, leaving it
    // permanently stuck true with nothing actually running.
    require_active_session(&state)?;

    // Resuming from a pause must skip the breakpoint check while the
    // session is still sitting on the line it paused at: `is_at_breakpoint()`
    // checks the instruction about to run, which is still that line
    // (pausing doesn't advance `pc`). Without the skip, the very first
    // iteration would immediately re-detect the same breakpoint and pause
    // again without ever stepping, making resume a permanent no-op. A flat
    // "skip exactly one iteration" isn't enough though: a load/store
    // produces multiple `Action`s (one per memory level) before `pc`
    // actually moves, and a pseudo-instruction expansion can share its
    // source line across several instructions — so what's remembered here
    // is the LINE, and the loop keeps skipping the check for as long as
    // it's still on that line (see `should_check_breakpoint`), not just
    // for one step.
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

    // Capture `pc` BEFORE stepping, same as `run_auto_step_loop`: the
    // underlying simulator advances its internal pc to the NEXT instruction
    // as part of completing the current one, so reading it after
    // `step_action()` would give the wrong (next) instruction's address.
    let pc = session.pc();
    if let Some(action) = session.step_action() {
        // Check whether this step left the session finished while still
        // holding the guard (cheap, no emit here), then drop the guard
        // before emitting anything — same lock-drop-before-emit discipline
        // as `run_auto_step_loop` and the rest of this file.
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

    // Same pre-step pc capture as `run_auto_step_loop` and `step_action`.
    let pc = session.pc();
    let actions = session.step_instruction();
    // Check finished status while still holding the guard (no emit here),
    // same lock-drop-before-emit discipline as `step_action`.
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

#[tauri::command]
pub fn get_registers(state: tauri::State<SessionState>) -> Result<RegisterSnapshot, String> {
    snapshot_registers(&state)
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SimEvent {
    RegWrite { register: u8, value: u64, source_line: usize },
    FpRegWrite { register: u8, value: f64, source_line: usize },
    CacheAccess { level_index: usize, outcome: String, source_line: usize },
    RamAccess { outcome: String, source_line: usize },
    DiskAccess { outcome: String, source_line: usize },
    TlbAccess { outcome: String, source_line: usize },
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

pub fn action_to_sim_event(action: &Action) -> SimEvent {
    let source_line = action.source_line;
    match &action.kind {
        ActionKind::RegisterWrite { register, value } => {
            SimEvent::RegWrite { register: register.0, value: *value, source_line }
        }
        ActionKind::FpRegisterWrite { register, value } => {
            SimEvent::FpRegWrite { register: register.0, value: *value, source_line }
        }
        ActionKind::MemoryLevel(level_result) => match level_result.level {
            LevelId::Tlb => SimEvent::TlbAccess { outcome: outcome_str(level_result.outcome), source_line },
            LevelId::Cache(index) => {
                SimEvent::CacheAccess { level_index: index, outcome: outcome_str(level_result.outcome), source_line }
            }
            LevelId::Ram => SimEvent::RamAccess { outcome: outcome_str(level_result.outcome), source_line },
            LevelId::Disk => SimEvent::DiskAccess { outcome: outcome_str(level_result.outcome), source_line },
        },
        ActionKind::NoOp => SimEvent::NoOp { source_line },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::simulation::memory::hierarchy::{LevelId, LevelOutcome, LevelResult};
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
            kind: ActionKind::MemoryLevel(LevelResult { level: LevelId::Cache(1), outcome: LevelOutcome::Hit }),
            source_line: 9,
            instruction_complete: false,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "cache_access");
        assert_eq!(json["levelIndex"], 1);
        assert_eq!(json["outcome"], "hit");
    }

    #[test]
    fn ram_miss_translates_to_ram_access_event() {
        let action = Action {
            kind: ActionKind::MemoryLevel(LevelResult { level: LevelId::Ram, outcome: LevelOutcome::Hit }),
            source_line: 9,
            instruction_complete: false,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "ram_access");
    }

    #[test]
    fn tlb_result_translates_to_tlb_access_event() {
        let action = Action {
            kind: ActionKind::MemoryLevel(LevelResult { level: LevelId::Tlb, outcome: LevelOutcome::Miss }),
            source_line: 9,
            instruction_complete: false,
        };
        let event = action_to_sim_event(&action);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "tlb_access");
        assert_eq!(json["outcome"], "miss");
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

    fn empty_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            hierarchy: vec![],
            tlb: TlbConfig { page_size: 4096, page_table_levels: 1, page_walk_start: "RAM".into(), entries: 4, associativity: 4, replacement: TlbReplacementPolicy::Lru },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
            disk: DiskConfig { latency: 10000 },
        }
    }

    fn empty_codegen() -> crate::models::interpreter::riscv::codegen::ir::CodegenOutput {
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
        assert!(session.is_finished()); // empty program
        assert!(session.step_action().is_none());
    }

    #[test]
    fn session_pc_delegates_to_the_inner_simulator() {
        let session = build_session(Base::Rv32i, empty_codegen(), empty_hierarchy_config(), 64, 64, vec![]);
        assert_eq!(session.pc(), 0x1000);
    }

    #[test]
    fn begin_run_returns_a_new_generation_on_every_call() {
        let state = SessionState::default();
        let g1 = state.begin_run();
        let g2 = state.begin_run();
        // Every call to begin_run represents a new logical "run" — whether
        // it's resume_simulation after a pause or a fresh start_simulation —
        // so any earlier auto-run thread (still holding the old generation)
        // can recognize it's stale and exit, even if it never observed
        // `running` flip to false in between.
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
        // Mirrors the exact ordering `run_auto_step_loop` now uses: read
        // `session.pc()` BEFORE `step_action()`, not after. The underlying
        // simulator already advances its internal pc to the NEXT
        // instruction as part of completing the current one, so reading pc
        // after stepping would give the wrong (next) instruction's address.
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
        let action = session.step_action().unwrap();
        assert!(action.instruction_complete);

        // The FIRST InstructionComplete event's pc must equal the program's
        // text_base (the instruction that just completed), not text_base+4
        // (the instruction the simulator has already advanced to).
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

    // --- Finding 1 / round-2 finding 5: resume-from-breakpoint skip semantics ---

    #[test]
    fn should_check_breakpoint_skips_only_while_current_line_matches_the_remembered_line() {
        // Still on the remembered "paused at" line: skip the check.
        assert!(!should_check_breakpoint(Some(1), Some(1)));
        // Moved to a different line: check resumes.
        assert!(should_check_breakpoint(Some(1), Some(2)));
        // Session finished (no current line) while a skip was still armed:
        // check resumes (there's nothing left to spuriously re-pause on,
        // and `is_finished()` is handled separately anyway).
        assert!(should_check_breakpoint(Some(1), None));
        // A fresh start (`skip_while_line == None`) always checks, no
        // matter what the current line is, matching standard debugger
        // behavior.
        assert!(should_check_breakpoint(None, Some(1)));
        assert!(should_check_breakpoint(None, None));
    }

    #[test]
    fn stepping_past_a_breakpoint_while_the_skip_line_matches_still_executes_the_action() {
        // Two instructions; breakpoint on the first. This mirrors exactly
        // what run_auto_step_loop observes right after pausing at a
        // breakpoint: pc hasn't moved, so is_at_breakpoint() alone (without
        // stepping) is still true.
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

        // Confirm the session is sitting exactly at the breakpoint, the way
        // run_auto_step_loop would see it immediately after pausing.
        assert!(session.is_at_breakpoint(), "checking is_at_breakpoint() alone (without stepping) must still report true");

        let skip_while_line = session.current_source_line();
        assert_eq!(skip_while_line, Some(1));

        // should_check_breakpoint(Some(1), Some(1)) is false: the loop-level
        // fix means this iteration does NOT re-check is_at_breakpoint() and
        // goes ahead and steps instead.
        assert!(!should_check_breakpoint(skip_while_line, session.current_source_line()));
        let action = session.step_action();
        assert!(action.is_some(), "the skip iteration must actually execute the instruction it paused on, not pause again");
    }

    #[test]
    fn resuming_on_a_multi_action_load_or_store_line_completes_the_whole_instruction_before_any_recheck() {
        // Round-2 finding 5 regression test: a `lw` against a zero-level
        // hierarchy (straight to RAM) produces TWO actions — a
        // `MemoryLevel` action, then the completing `RegisterWrite` — with
        // `pc` (and so `current_source_line()`) unchanged between them.
        // resume_simulation's remembered skip line must survive across
        // BOTH actions, not just the first, or the loop would spuriously
        // re-pause after only the memory-level action, before the
        // instruction (and its register write / pc advance) completes.
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

        // Mirrors resume_simulation: remember the line we're paused at.
        let mut skip_while_line = session.current_source_line();
        assert_eq!(skip_while_line, Some(1));
        assert!(session.is_at_breakpoint());

        // Iteration 1: the memory-level action. Must be skipped (not
        // re-paused) and pc/source line must NOT have moved yet.
        let current_line = session.current_source_line();
        if skip_while_line.is_some() && skip_while_line != current_line {
            skip_while_line = None;
        }
        assert!(!should_check_breakpoint(skip_while_line, current_line), "must still be skipping on the memory-level action");
        let first = session.step_action().unwrap();
        assert!(matches!(first.kind, ActionKind::MemoryLevel(_)));
        assert!(!first.instruction_complete, "lw against RAM-only hierarchy resolves in one memory-level step");
        assert_eq!(session.current_source_line(), Some(1), "pc must not have advanced yet — the instruction isn't complete");

        // Iteration 2: the completing RegisterWrite action, STILL on the
        // same source line (pc only advances as part of THIS action).
        // This is exactly the case round 1's "skip exactly one iteration"
        // mechanism got wrong: it would have already re-armed the check
        // here and spuriously paused again before this action ran.
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

        // Iteration 3: now on a new line — the skip must have permanently
        // un-armed, so normal breakpoint checking resumes.
        let current_line = session.current_source_line();
        if skip_while_line.is_some() && skip_while_line != current_line {
            skip_while_line = None;
        }
        assert_eq!(skip_while_line, None, "skip must be permanently cleared once the line changes");
        assert!(should_check_breakpoint(skip_while_line, current_line), "normal breakpoint checking must resume once past the original line");
    }

    // --- Finding 4: resume_simulation with no active session ---

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

    // --- Finding 3: manual stepping to the end of a program ---

    #[test]
    fn step_action_leaves_a_finished_session_detectable_after_the_final_step() {
        // A single-instruction program: stepping it once completes the
        // program. The controller command (not exercised directly here
        // since it needs an AppHandle) checks `session.is_finished()`
        // right after this step, still under the guard, and emits
        // SimulationPaused{reason:"end"} once dropped — this test proves
        // the underlying condition it relies on.
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
        let action = session.step_action();
        assert!(action.is_some(), "the step that completes the program must still return the action");
        assert!(session.is_finished(), "the session must report finished immediately after the step that completed it");

        // The event the controller is expected to emit as a result.
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

    #[test]
    fn snapshot_registers_errors_with_no_active_session() {
        let state = SessionState::default();
        let result = snapshot_registers(&state);
        assert_eq!(result.unwrap_err(), "no active simulation session");
    }
}
