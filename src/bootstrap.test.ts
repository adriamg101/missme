import { describe, it, expect } from "vitest";
import { createPinia, setActivePinia } from "pinia";

describe("bootstrap", () => {
  it("can create and activate a pinia instance", () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    expect(pinia).toBeTruthy();
  });
});
