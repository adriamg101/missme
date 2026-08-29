import { defineStore } from "pinia";

// Matches CodegenConfig::default() in
// src-tauri/src/models/interpreter/riscv/mod.rs. Fixed by convention, not
// user-editable — exposed as a getter, not state, so there's no setter to
// call by mistake.
const TEXT_BASE = 0x00400000;

interface CompileConfigState {
  dataBase: number;
  // Populated after a successful compile, from the actual codegen output —
  // lets validation (e.g. the .data/.text collision check) use real sizes
  // instead of a guess. Null until the first successful compile.
  lastCompiledTextSize: number | null;
  lastCompiledDataSize: number | null;
}

export const useCompileConfigStore = defineStore("compileConfig", {
  state: (): CompileConfigState => ({
    dataBase: 0x10000000,
    lastCompiledTextSize: null,
    lastCompiledDataSize: null,
  }),
  getters: {
    textBase: (): number => TEXT_BASE,
  },
  actions: {
    setDataBase(value: number) {
      this.dataBase = value;
    },
    setCompiledSizes(textSize: number, dataSize: number) {
      this.lastCompiledTextSize = textSize;
      this.lastCompiledDataSize = dataSize;
    },
  },
});
