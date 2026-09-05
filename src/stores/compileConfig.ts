import { defineStore } from "pinia";

// Matches the default setup in src-tauri/src/models/interpreter/riscv/mod.rs.
// This is fixed by convention, so it is exposed as a read only value.
const TEXT_BASE = 0x00400000;

interface CompileConfigState {
  dataBase: number;
  // Filled in after a successful compile, using the real output sizes so
  // overlap checks can use actual numbers instead of a guess.
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
