// src/components/hierarchy-config/CacheConfigForm.test.ts
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import CacheConfigForm from "./CacheConfigForm.vue";
import { defaultCacheConfig } from "../../lib/hierarchy/model";

describe("CacheConfigForm", () => {
  it("emits an update patch when block size changes", async () => {
    const { emitted } = render(CacheConfigForm, {
      props: { config: defaultCacheConfig(), cacheSizeExp: 15, isInstructionCache: false, isa: "RV32I" },
    });
    await fireEvent.click(screen.getAllByLabelText("Increment")[0]);
    expect(emitted().update[0]).toEqual([{ blockSize: 128 }]);
  });

  it("clamps associativity to the number of entries (cacheSizeExp), regardless of block size", async () => {
    const config = { ...defaultCacheConfig(), blockSize: 64, associativity: 1024 };
    const { emitted } = render(CacheConfigForm, {
      props: { config, cacheSizeExp: 10 /* 1024 entries */, isInstructionCache: false, isa: "RV32I" },
    });
    await fireEvent.click(screen.getAllByLabelText("Increment")[1]);
    expect(emitted().update).toBeUndefined();
  });

  it("hides write policy and write-allocate for instruction caches", () => {
    render(CacheConfigForm, {
      props: { config: defaultCacheConfig(), cacheSizeExp: 15, isInstructionCache: true, isa: "RV32I" },
    });
    expect(screen.queryByLabelText("Write policy")).toBeNull();
    expect(screen.queryByLabelText("Write allocate")).toBeNull();
  });

  it("shows write policy and write-allocate for data/unified caches", () => {
    render(CacheConfigForm, {
      props: { config: defaultCacheConfig(), cacheSizeExp: 15, isInstructionCache: false, isa: "RV32I" },
    });
    expect(screen.getByLabelText("Write policy")).toBeTruthy();
    expect(screen.getByLabelText("Write allocate")).toBeTruthy();
  });

  it("does not let block size go below the ISA's word size (4 bytes on RV32I)", async () => {
    const config = { ...defaultCacheConfig(), blockSize: 4 };
    const { emitted } = render(CacheConfigForm, {
      props: { config, cacheSizeExp: 15, isInstructionCache: false, isa: "RV32I" },
    });
    await fireEvent.click(screen.getAllByLabelText("Decrement")[0]);
    expect(emitted().update).toBeUndefined();
  });

  it("does not let block size go below the ISA's word size (8 bytes on RV64I)", async () => {
    const config = { ...defaultCacheConfig(), blockSize: 8 };
    const { emitted } = render(CacheConfigForm, {
      props: { config, cacheSizeExp: 15, isInstructionCache: false, isa: "RV64I" },
    });
    await fireEvent.click(screen.getAllByLabelText("Decrement")[0]);
    expect(emitted().update).toBeUndefined();
  });
});
