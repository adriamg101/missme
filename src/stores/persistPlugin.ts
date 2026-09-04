import type { PiniaPluginContext } from "pinia";

const STORAGE_KEY = "missme.hierarchyConfig";

export function persistPlugin({ store }: PiniaPluginContext) {
  if (store.$id !== "hierarchyConfig") return;

  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved) {
    try {
      store.$patch(JSON.parse(saved));
    } catch {
      // the saved data is broken or from an old format, so ignore it and keep the defaults
    }
  }

  // Save immediately when a change happens, instead of waiting and batching
  // it with other updates. Otherwise a read that happens right after a write
  // (like in the save-and-reload test, and during normal app navigation)
  // could run before the save actually finishes.
  store.$subscribe(
    (_mutation, state) => {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    },
    { flush: "sync" },
  );
}
