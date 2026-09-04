import "@testing-library/jest-dom";

// Starting with Node 24, Node itself provides an experimental `localStorage`
// saved to a file on disk. Since that already exists before Vitest's fake
// browser environment (jsdom) starts up, Vitest assumes it's already set up
// and skips replacing it with jsdom's own working `localStorage`. That
// leaves the global `localStorage` as Node's version, which does nothing
// unless a special command line flag was used. Vitest keeps the jsdom
// environment available on a global variable, so this grabs jsdom's real
// `localStorage` from there and puts it in place of Node's, so both the app
// code and `localStorage.clear()` in tests use the one that actually works.
const jsdomInstance = (globalThis as unknown as { jsdom?: { window: Window } }).jsdom;
if (jsdomInstance?.window?.localStorage) {
  Object.defineProperty(globalThis, "localStorage", {
    value: jsdomInstance.window.localStorage,
    configurable: true,
    writable: true,
  });
}
