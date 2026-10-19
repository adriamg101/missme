import "@testing-library/jest-dom";

// Newer versions of Node provide their own local storage that does nothing
// by default, and the test tool mistakes it for a working one. This swaps in the real one.
const jsdomInstance = (globalThis as unknown as { jsdom?: { window: Window } }).jsdom;
if (jsdomInstance?.window?.localStorage) {
  Object.defineProperty(globalThis, "localStorage", {
    value: jsdomInstance.window.localStorage,
    configurable: true,
    writable: true,
  });
}
