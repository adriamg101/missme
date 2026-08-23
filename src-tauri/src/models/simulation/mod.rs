pub mod cpu;
pub mod memory;

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

/// Executes a compiled program one atomic action at a time. A non-memory
/// instruction is one action; a load/store is one action per memory level
/// touched, with `instruction_complete` only true on the action that
/// resolves the whole instruction.
pub trait CpuSimulator {
    fn step_action(&mut self) -> Action;
    fn is_finished(&self) -> bool;
    fn pc(&self) -> u64;
    fn register(&self, id: RegisterId) -> u64;
    fn fp_register(&self, id: RegisterId) -> f64;
}
