import { defineStore } from "pinia";

interface CompileConfigState {
  textBase: number;
  dataBase: number;
  // Filled in after a successful compile, using the real output sizes so
  // overlap checks can use actual numbers instead of a guess.
  lastCompiledTextSize: number | null;
  lastCompiledDataSize: number | null;
}

export const useCompileConfigStore = defineStore("compileConfig", {
  state: (): CompileConfigState => ({
    textBase: 0x00400000,
    dataBase: 0x10000000,
    lastCompiledTextSize: null,
    lastCompiledDataSize: null,
  }),
  actions: {
    setTextBase(value: number) {
      this.textBase = value;
    },
    setDataBase(value: number) {
      this.dataBase = value;
    },
    setCompiledSizes(textSize: number, dataSize: number) {
      this.lastCompiledTextSize = textSize;
      this.lastCompiledDataSize = dataSize;
    },
  },
});
