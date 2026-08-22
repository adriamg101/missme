// src/components/hierarchy-config/CacheConfigForm.test.ts
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import CacheConfigForm from "./CacheConfigForm.vue";
import { defaultCacheConfig } from "../../lib/hierarchy/model";

describe("CacheConfigForm", () => {
  it("emits an update patch when block size changes", async () => {
    const { emitted } = render(CacheConfigForm, {
      props: { config: defaultCacheConfig(), cacheSizeExp: 15, isInstructionCache: false },
    });
    await fireEvent.click(screen.getByLabelText("Incrementar"));
    expect(emitted().update[0]).toEqual([{ blockSize: 128 }]);
  });

  it("clamps associativity to the cache size / block size maximum", async () => {
    const config = { ...defaultCacheConfig(), blockSize: 64 };
    render(CacheConfigForm, { props: { config, cacheSizeExp: 10 /* 1KB cache */, isInstructionCache: false } }); // max = 1024/64 = 16
    const input = screen.getByLabelText("Associativity") as HTMLInputElement;
    expect(input.max).toBe("16");
  });

  it("hides write policy and write-allocate for instruction caches", () => {
    render(CacheConfigForm, { props: { config: defaultCacheConfig(), cacheSizeExp: 15, isInstructionCache: true } });
    expect(screen.queryByLabelText("Write policy")).toBeNull();
    expect(screen.queryByLabelText("Write allocate")).toBeNull();
  });

  it("shows write policy and write-allocate for data/unified caches", () => {
    render(CacheConfigForm, { props: { config: defaultCacheConfig(), cacheSizeExp: 15, isInstructionCache: false } });
    expect(screen.getByLabelText("Write policy")).toBeTruthy();
    expect(screen.getByLabelText("Write allocate")).toBeTruthy();
  });
});
