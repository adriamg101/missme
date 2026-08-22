import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import HierarchyConfigPanel from "./HierarchyConfigPanel.vue";

beforeEach(() => setActivePinia(createPinia()));

describe("HierarchyConfigPanel", () => {
  it("renders the global config bar and the (initially empty) tab strip together", () => {
    render(HierarchyConfigPanel);
    expect(screen.getByLabelText("Virtual Memory")).toBeTruthy();
    expect(screen.queryByRole("tab")).toBeNull();
  });

  it("creating a level via the slider immediately produces a matching tab", async () => {
    const { container } = render(HierarchyConfigPanel);
    const slot = container.querySelectorAll("[data-slider-position]")[5];
    await fireEvent.click(slot!);
    expect(screen.getByText("L1")).toBeTruthy();
  });
});
