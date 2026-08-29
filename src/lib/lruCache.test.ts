import { describe, it, expect } from "vitest";
import { touchAndPrune } from "./lruCache";

describe("touchAndPrune", () => {
  it("touches a key by moving it to the end of iteration order", () => {
    // Seed cache with keys 1, 2, 3
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Touch key 1 (re-insert it to move to end)
    touchAndPrune(cache, [1], 3);

    // Verify key 1 is now at the end of iteration order
    // by checking that the first key is 2 (not 1, which was touched)
    const keys = Array.from(cache.keys());
    expect(keys).toEqual([2, 3, 1]);
  });

  it("evicts stale keys (not recently touched) when cache exceeds maxSize", () => {
    // Seed cache with keys 1, 2, 3
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Touch key 1 (move to end)
    touchAndPrune(cache, [1], 3);

    // Add a new key 4 (simulating a new block fetch)
    cache.set(4, "d");

    // Now prune with maxSize=3; size is 4, so 1 key must be evicted
    touchAndPrune(cache, [4], 3);

    // Key 2 (the oldest never-touched key) should be evicted
    // Keys 1, 3, 4 should survive (1 was touched, 3 and 4 are newer)
    expect(cache.has(2)).toBe(false);
    expect(cache.has(1)).toBe(true);
    expect(cache.has(3)).toBe(true);
    expect(cache.has(4)).toBe(true);
    expect(cache.size).toBe(3);
  });

  it("demonstrates the difference: without touch, the oldest key by insertion order is evicted", () => {
    // Seed cache with keys 1, 2, 3
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // DON'T touch any key (empty touched list)
    touchAndPrune(cache, [], 3);

    // Add a new key 4
    cache.set(4, "d");

    // Prune with no touch history — pure insertion-order eviction
    touchAndPrune(cache, [], 3);

    // Key 1 (the true oldest by insertion order) is evicted
    expect(cache.has(1)).toBe(false);
    expect(cache.has(2)).toBe(true);
    expect(cache.has(3)).toBe(true);
    expect(cache.has(4)).toBe(true);
    expect(cache.size).toBe(3);
  });

  it("does not evict when cache size is at or under maxSize", () => {
    // Seed cache with keys 1, 2, 3
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Touch key 1 and prune with maxSize=3 (size matches max)
    touchAndPrune(cache, [1], 3);

    // No keys should be evicted since size (3) <= maxSize (3)
    expect(cache.size).toBe(3);
    expect(cache.has(1)).toBe(true);
    expect(cache.has(2)).toBe(true);
    expect(cache.has(3)).toBe(true);
  });

  it("evicts multiple keys when cache is significantly over maxSize", () => {
    // Seed cache with keys 1-10
    const cache = new Map<number, string>();
    for (let i = 1; i <= 10; i++) {
      cache.set(i, `val${i}`);
    }

    // Touch keys 8, 9, 10 (move them to end)
    touchAndPrune(cache, [8, 9, 10], 5);

    // Prune with maxSize=5; size is 10, so 5 keys must be evicted
    // Keys 1-5 (the oldest, never touched) should be evicted
    // Keys 6, 7, 8, 9, 10 should survive (size = 5, at or above threshold)
    expect(cache.size).toBe(5);
    for (let i = 1; i <= 5; i++) {
      expect(cache.has(i)).toBe(false);
    }
    expect(cache.has(6)).toBe(true);
    expect(cache.has(7)).toBe(true);
    expect(cache.has(8)).toBe(true);
    expect(cache.has(9)).toBe(true);
    expect(cache.has(10)).toBe(true);
  });

  it("handles touching a key that doesn't exist (no-op)", () => {
    // Seed cache with keys 1, 2, 3
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Try to touch key 99 (doesn't exist)
    touchAndPrune(cache, [99], 3);

    // Cache should be unchanged
    expect(cache.size).toBe(3);
    expect(Array.from(cache.keys())).toEqual([1, 2, 3]);
  });

  it("handles touching multiple keys in a single call", () => {
    // Seed cache with keys 1, 2, 3, 4, 5
    const cache = new Map<number, string>();
    for (let i = 1; i <= 5; i++) {
      cache.set(i, `val${i}`);
    }

    // Touch keys 1 and 3 (reorder them to the end)
    touchAndPrune(cache, [1, 3], 5);

    // Keys 1 and 3 should now be at the end
    const keys = Array.from(cache.keys());
    expect(keys).toEqual([2, 4, 5, 1, 3]);
  });
});
