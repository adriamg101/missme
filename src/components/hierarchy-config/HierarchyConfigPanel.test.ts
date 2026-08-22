import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import HierarchyConfigPanel from "./HierarchyConfigPanel.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

beforeEach(() => setActivePinia(createPinia()));

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

  it("clicking New file resets the store directly when there's nothing to lose", async () => {
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("Nueva configuración"));
    expect(screen.queryByText("Hay una jerarquía sin guardar en la configuración. ¿Qué quieres hacer?")).toBeNull();
    expect(useHierarchyConfigStore().hierarchy).toHaveLength(0);
  });

  it("clicking New file with an existing hierarchy asks for confirmation first", async () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("Nueva configuración"));
    expect(screen.getByText("Hay una jerarquía sin guardar en la configuración. ¿Qué quieres hacer?")).toBeTruthy();
    expect(store.hierarchy).toHaveLength(1); // unchanged until the user picks an option
  });

  it("choosing 'Sobrescribir sin guardar' resets the store to defaults", async () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.setVmEnabled(true);
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("Nueva configuración"));
    await fireEvent.click(screen.getByText("Sobrescribir sin guardar"));
    expect(store.hierarchy).toHaveLength(0);
    expect(store.vmEnabled).toBe(false);
  });

  it("choosing 'Cancelar' leaves the hierarchy untouched and closes the modal", async () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("Nueva configuración"));
    await fireEvent.click(screen.getByText("Cancelar"));
    expect(screen.queryByText("Hay una jerarquía sin guardar en la configuración. ¿Qué quieres hacer?")).toBeNull();
    expect(store.hierarchy).toHaveLength(1);
  });
});
