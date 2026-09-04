import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import HierarchyConfigPanel from "./HierarchyConfigPanel.vue";

beforeEach(() => {
  setActivePinia(createPinia());
});

describe("HierarchyConfigPanel", () => {
  it("renders the global config bar and the tab strip (RAM only) together", () => {
    render(HierarchyConfigPanel);
    expect(screen.getByLabelText("Virtual Memory")).toBeTruthy();
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["RAM"]);
  });

  it("creating a level via the slider immediately produces a matching tab", async () => {
    const { container } = render(HierarchyConfigPanel);
    const slot = container.querySelectorAll("[data-slider-position]")[5];
    await fireEvent.click(slot!);
    expect(screen.getByText("L1")).toBeTruthy();
  });
});
