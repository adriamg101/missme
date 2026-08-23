import type { PiniaPluginContext } from "pinia";

const STORAGE_KEY = "missme.hierarchyConfig";

export function persistPlugin({ store }: PiniaPluginContext) {
  if (store.$id !== "hierarchyConfig") return;

  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved) {
    try {
      store.$patch(JSON.parse(saved));
    } catch {
      // corrupted/old-format data: ignore and keep defaults
    }
  }

  // flush: "sync" so the write lands immediately when the mutation happens
  // (rather than batched on Vue's next tick) — otherwise a synchronous
  // read-immediately-after-write (as in the persistence round-trip test,
  // and in general app navigation) can race ahead of the write.
  store.$subscribe(
    (_mutation, state) => {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    },
    { flush: "sync" },
  );
}
