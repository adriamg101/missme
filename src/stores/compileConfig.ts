import { defineStore } from "pinia";

// Matches CodegenConfig::default() in
// src-tauri/src/models/interpreter/riscv/mod.rs. Fixed by convention, not
// user-editable — exposed as a getter, not state, so there's no setter to
// call by mistake.
const TEXT_BASE = 0x00400000;

interface CompileConfigState {
  dataBase: number;
}

export const useCompileConfigStore = defineStore("compileConfig", {
  state: (): CompileConfigState => ({
    dataBase: 0x10000000,
  }),
  getters: {
    textBase: (): number => TEXT_BASE,
  },
  actions: {
    setDataBase(value: number) {
      this.dataBase = value;
    },
  },
});
