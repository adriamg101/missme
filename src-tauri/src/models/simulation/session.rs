use std::collections::HashSet;

use super::{Action, CpuSimulator, RegisterId};

pub struct SimulationSession {
    simulator: Box<dyn CpuSimulator + Send>,
    breakpoints: HashSet<usize>,
}

impl SimulationSession {
    pub fn new(simulator: Box<dyn CpuSimulator + Send>, breakpoints: HashSet<usize>) -> Self {
        Self { simulator, breakpoints }
    }

    pub fn is_finished(&self) -> bool {
        self.simulator.is_finished()
    }

    pub fn pc(&self) -> u64 {
        self.simulator.pc()
    }

    pub fn register(&self, id: RegisterId) -> u64 {
        self.simulator.register(id)
    }

    pub fn fp_register(&self, id: RegisterId) -> f64 {
        self.simulator.fp_register(id)
    }

    pub fn memory(&self) -> &super::memory::hierarchy::MemoryHierarchySimulator {
        self.simulator.memory()
    }

    pub fn is_at_breakpoint(&self) -> bool {
        match self.simulator.current_source_line() {
            Some(line) => self.breakpoints.contains(&line),
            None => false,
        }
    }

    /// The source line of the instruction about to run.
    /// Exposed so the caller can remember where the simulation paused and decide when to check breakpoints again after resuming.
    pub fn current_source_line(&self) -> Option<usize> {
        self.simulator.current_source_line()
    }

    pub fn step_action(&mut self) -> Option<Action> {
        if self.simulator.is_finished() {
            return None;
        }
        Some(self.simulator.step_action())
    }

    pub fn step_instruction(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        loop {
            match self.step_action() {
                None => break,
                Some(action) => {
                    let complete = action.instruction_complete;
                    actions.push(action);
                    if complete {
                        break;
                    }
                }
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::codegen::ir::{CodegenOutput, IrEntry, MemoryLayout};
    use crate::models::interpreter::riscv::lexer::token::{InstructionKind, RegisterKind};
    use crate::models::interpreter::riscv::parser::ast::Operand;
    use crate::models::interpreter::riscv::Base;
    use crate::models::simulation::cpu::RiscvCpuSimulator;
    use crate::models::simulation::memory::backing_store::DataStore;
    use crate::models::simulation::memory::config::{DiskConfig, FrameAllocationPolicy, HierarchyConfig, RamConfig, TlbConfig, TlbReplacementPolicy};
    use crate::models::simulation::memory::hierarchy::MemoryHierarchySimulator;
    use crate::models::simulation::RegisterId;
    use std::collections::{HashMap, HashSet};

    fn no_cache_hierarchy_config() -> HierarchyConfig {
        HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            page_size: 4096,
            hierarchy: vec![],
            tlb: TlbConfig { page_walk_start: "RAM".into(), entries: 4, associativity: 4, replacement: TlbReplacementPolicy::Lru },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        }
    }

    fn mem() -> MemoryHierarchySimulator {
        MemoryHierarchySimulator::new(no_cache_hierarchy_config(), DataStore::new(64), DataStore::new(64), 0)
    }

    fn add_entry(address: u32, line: usize) -> IrEntry {
        IrEntry {
            address,
            kind: InstructionKind::Add,
            operands: vec![Operand::Register(RegisterKind::X(1)), Operand::Register(RegisterKind::X(2)), Operand::Register(RegisterKind::X(3))],
            bytecode: 0,
            source_line: line,
            source_column: 1,
        }
    }

    fn codegen(entries: Vec<IrEntry>) -> CodegenOutput {
        CodegenOutput {
            layout: MemoryLayout { text_base: 0x1000, data_base: 0x2000, labels: HashMap::new(), initial_data: vec![] },
            entries,
        }
    }

    fn sim(entries: Vec<IrEntry>) -> Box<dyn CpuSimulator + Send> {
        Box::new(RiscvCpuSimulator::new(Base::Rv32i, codegen(entries), mem()))
    }

    #[test]
    fn step_action_returns_none_once_finished() {
        let mut session = SimulationSession::new(sim(vec![]), HashSet::new());
        assert!(session.is_finished());
        assert!(session.step_action().is_none());
    }

    #[test]
    fn step_action_advances_and_returns_the_action() {
        let mut session = SimulationSession::new(sim(vec![add_entry(0x1000, 1)]), HashSet::new());

        // Every instruction starts by fetching it from memory first, one action that is not the final step yet.
        // A second call then finishes the fetch and actually runs the instruction.
        let first = session.step_action().unwrap();
        assert!(!first.instruction_complete);
        assert!(!session.is_finished());

        let second = session.step_action().unwrap();
        assert!(second.instruction_complete);
        assert!(session.is_finished());
    }

    #[test]
    fn is_at_breakpoint_is_true_only_for_a_configured_line_before_it_runs() {
        let session_with_bp = SimulationSession::new(sim(vec![add_entry(0x1000, 5)]), HashSet::from([5]));
        assert!(session_with_bp.is_at_breakpoint());

        let session_without_bp = SimulationSession::new(sim(vec![add_entry(0x1000, 5)]), HashSet::from([99]));
        assert!(!session_without_bp.is_at_breakpoint());
    }

    #[test]
    fn is_at_breakpoint_is_false_once_finished() {
        let session = SimulationSession::new(sim(vec![]), HashSet::from([1]));
        assert!(!session.is_at_breakpoint());
    }

    #[test]
    fn step_instruction_collects_every_action_for_one_instruction() {
        let mut session = SimulationSession::new(sim(vec![add_entry(0x1000, 1), add_entry(0x1004, 2)]), HashSet::new());
        let actions = session.step_instruction();
        // Fetching the instruction plus actually running it makes two steps
        // per instruction here, since no cache levels are configured.
        assert_eq!(actions.len(), 2);
        assert!(!actions[0].instruction_complete);
        assert!(actions[1].instruction_complete);
        assert!(!session.is_finished());
        assert_eq!(session.step_instruction().len(), 2);
        assert!(session.is_finished());
    }

    #[test]
    fn step_instruction_on_a_finished_session_returns_an_empty_vec() {
        let mut session = SimulationSession::new(sim(vec![]), HashSet::new());
        assert_eq!(session.step_instruction(), vec![]);
    }

    #[test]
    fn register_and_fp_register_read_through_to_the_simulator() {
        let mut session = SimulationSession::new(sim(vec![add_entry(0x1000, 1)]), HashSet::new());
        assert_eq!(session.register(RegisterId(0)), 0);
        session.step_instruction();
        // reading a register before anything was ever written to it should
        // not crash
        let _ = session.register(RegisterId(31));
        let _ = session.fp_register(RegisterId(31));
    }

    #[test]
    fn memory_exposes_the_underlying_memory_simulator() {
        let session = SimulationSession::new(sim(vec![add_entry(0x1000, 1)]), HashSet::new());
        // This read only call should succeed without crashing, showing this method really reaches a working simulator underneath.
        assert!(session.memory().cache_level_info(0, crate::models::simulation::memory::hierarchy::CacheSide::Unified).is_none());
    }
}
