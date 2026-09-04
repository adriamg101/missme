import { defineStore } from "pinia";

// Matches the default codegen setup in
// src-tauri/src/models/interpreter/riscv/mod.rs. This is fixed by
// convention and not something the user can edit, so it's exposed as a
// read-only value rather than editable state.
const TEXT_BASE = 0x00400000;

interface CompileConfigState {
  dataBase: number;
  // Filled in after a successful compile, using the real output sizes so
  // checks (like making sure .data and .text don't overlap) can use actual
  // numbers instead of a guess. Stays empty until the first successful compile.
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
