import { describe, it, expect } from "vitest";
import { touchAndPrune } from "./lruCache";

describe("touchAndPrune", () => {
  it("touches a key by moving it to the end of iteration order", () => {
    // Start with keys 1, 2, 3 in the cache
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Touch key 1, which moves it to the end
    touchAndPrune(cache, [1], 3);

    // Check key 1 is now last, by confirming the first key is 2, not 1,
    // since 1 was touched
    const keys = Array.from(cache.keys());
    expect(keys).toEqual([2, 3, 1]);
  });

  it("evicts stale keys (not recently touched) when cache exceeds maxSize", () => {
    // Start with keys 1, 2, 3 in the cache
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Touch key 1 to move it to the end
    touchAndPrune(cache, [1], 3);

    // Add a new key 4, as if a new block was fetched
    cache.set(4, "d");

    // Prune with a max size of 3. The cache has 4 keys, so 1 must be removed
    touchAndPrune(cache, [4], 3);

    // Key 2, the oldest key that was never touched, should be removed
    // Keys 1, 3, 4 should survive, since 1 was touched and 3 and 4 are newer
    expect(cache.has(2)).toBe(false);
    expect(cache.has(1)).toBe(true);
    expect(cache.has(3)).toBe(true);
    expect(cache.has(4)).toBe(true);
    expect(cache.size).toBe(3);
  });

  it("demonstrates the difference: without touch, the oldest key by insertion order is evicted", () => {
    // Start with keys 1, 2, 3 in the cache
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Don't touch any key this time
    touchAndPrune(cache, [], 3);

    // Add a new key 4
    cache.set(4, "d");

    // Prune with no touch history, so keys are removed purely by the order
    // they were added
    touchAndPrune(cache, [], 3);

    // Key 1, the oldest by the order it was added, is removed
    expect(cache.has(1)).toBe(false);
    expect(cache.has(2)).toBe(true);
    expect(cache.has(3)).toBe(true);
    expect(cache.has(4)).toBe(true);
    expect(cache.size).toBe(3);
  });

  it("does not evict when cache size is at or under maxSize", () => {
    // Start with keys 1, 2, 3 in the cache
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Touch key 1 and prune with a max size of 3, which matches the current
    // size
    touchAndPrune(cache, [1], 3);

    // No keys should be removed, since the size (3) is not above the max (3)
    expect(cache.size).toBe(3);
    expect(cache.has(1)).toBe(true);
    expect(cache.has(2)).toBe(true);
    expect(cache.has(3)).toBe(true);
  });

  it("evicts multiple keys when cache is significantly over maxSize", () => {
    // Start with keys 1 through 10 in the cache
    const cache = new Map<number, string>();
    for (let i = 1; i <= 10; i++) {
      cache.set(i, `val${i}`);
    }

    // Touch keys 8, 9, 10 to move them to the end
    touchAndPrune(cache, [8, 9, 10], 5);

    // Prune with a max size of 5. The cache has 10 keys, so 5 must be removed
    // Keys 1 through 5, the oldest ones that were never touched, should be removed
    // Keys 6 through 10 should survive
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
    // Start with keys 1, 2, 3 in the cache
    const cache = new Map<number, string>();
    cache.set(1, "a");
    cache.set(2, "b");
    cache.set(3, "c");

    // Try to touch key 99, which doesn't exist
    touchAndPrune(cache, [99], 3);

    // The cache should stay the same
    expect(cache.size).toBe(3);
    expect(Array.from(cache.keys())).toEqual([1, 2, 3]);
  });

  it("handles touching multiple keys in a single call", () => {
    // Start with keys 1 through 5 in the cache
    const cache = new Map<number, string>();
    for (let i = 1; i <= 5; i++) {
      cache.set(i, `val${i}`);
    }

    // Touch keys 1 and 3 to move them to the end
    touchAndPrune(cache, [1, 3], 5);

    // Keys 1 and 3 should now be at the end
    const keys = Array.from(cache.keys());
    expect(keys).toEqual([2, 4, 5, 1, 3]);
  });
});
