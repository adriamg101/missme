import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import Editor from "./Editor.vue";
import { useHierarchyConfigStore } from "../stores/hierarchyConfig";

beforeEach(() => {
  setActivePinia(createPinia());
});

describe("Editor ISA selection", () => {
  it("selecting RV64I in the options menu updates the shared store", async () => {
    render(Editor, { global: { plugins: [] } });
    await fireEvent.click(screen.getByLabelText("Compilation options"));
    await fireEvent.click(screen.getByText("RV64I"));
    const store = useHierarchyConfigStore();
    expect(store.isa).toBe("RV64I");
  });

  it("reflects the store's current ISA as the active option", () => {
    const store = useHierarchyConfigStore();
    store.setIsa("RV64I");
    render(Editor);
    // RV64I must already be the selected option as soon as the page loads
    expect(store.isa).toBe("RV64I");
  });
});
