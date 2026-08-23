import "@testing-library/jest-dom";

// Node >= 24 ships its own experimental global `localStorage` (backed by a
// SQLite file). Because that property already exists on `globalThis` before
// Vitest's jsdom environment runs, Vitest's global-populating logic treats it
// as "already provided by the host" and skips copying jsdom's real
// `window.localStorage` over it — leaving `globalThis.localStorage` as
// Node's own accessor, which resolves to `undefined` unless
// `--localstorage-file` was passed on the CLI. Vitest stashes the live jsdom
// instance on `globalThis.jsdom`; we pull the real localStorage off of it and
// force it onto `globalThis` so store code (and `localStorage.clear()` in
// tests) gets jsdom's implementation instead of Node's broken one.
const jsdomInstance = (globalThis as unknown as { jsdom?: { window: Window } }).jsdom;
if (jsdomInstance?.window?.localStorage) {
  Object.defineProperty(globalThis, "localStorage", {
    value: jsdomInstance.window.localStorage,
    configurable: true,
    writable: true,
  });
}
