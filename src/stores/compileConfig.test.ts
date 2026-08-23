import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useCompileConfigStore } from "./compileConfig";

beforeEach(() => setActivePinia(createPinia()));

describe("compileConfig store", () => {
  it("defaults dataBase to the conventional RISC-V data segment address", () => {
    expect(useCompileConfigStore().dataBase).toBe(0x10000000);
  });

  it("exposes a fixed textBase matching the backend's CodegenConfig default", () => {
    expect(useCompileConfigStore().textBase).toBe(0x00400000);
  });

  it("setDataBase updates dataBase", () => {
    const store = useCompileConfigStore();
    store.setDataBase(0x20000000);
    expect(store.dataBase).toBe(0x20000000);
  });
});
