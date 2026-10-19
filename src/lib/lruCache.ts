export function touchAndPrune<K, V>(cache: Map<K, V>, touchedKeys: K[], maxSize: number): void {
  for (const key of touchedKeys) {
    if (cache.has(key)) {
      const value = cache.get(key) as V;
      cache.delete(key);
      cache.set(key, value);
    }
  }
  if (cache.size > maxSize) {
    const excess = cache.size - maxSize;
    const staleKeys = Array.from(cache.keys()).slice(0, excess);
    for (const key of staleKeys) cache.delete(key);
  }
}
