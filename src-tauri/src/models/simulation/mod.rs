pub mod cpu;
pub mod memory;
pub mod session;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterId(pub u8);

#[derive(Debug, Clone, PartialEq)]
pub enum ActionKind {
    RegisterWrite { register: RegisterId, value: u64 },
    FpRegisterWrite { register: RegisterId, value: f64 },
    MemoryLevel(memory::hierarchy::LevelResult),
    NoOp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub kind: ActionKind,
    pub source_line: usize,
    pub instruction_complete: bool,
}

/// Runs a compiled program one small step at a time. A load or store takes one step per memory level it has to check.
/// instruction_complete is only true on the step that finishes the whole instruction.
pub trait CpuSimulator {
    fn step_action(&mut self) -> Action;
    fn is_finished(&self) -> bool;
    fn pc(&self) -> u64;
    fn register(&self, id: RegisterId) -> u64;
    fn fp_register(&self, id: RegisterId) -> f64;
    fn memory(&self) -> &memory::hierarchy::MemoryHierarchySimulator;
    fn current_source_line(&self) -> Option<usize>;
}
