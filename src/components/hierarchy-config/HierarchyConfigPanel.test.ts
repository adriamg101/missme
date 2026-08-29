import { describe, it, expect, beforeEach, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import HierarchyConfigPanel from "./HierarchyConfigPanel.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const mkdirMock = vi.fn().mockResolvedValue(undefined);
const readDirMock = vi.fn().mockResolvedValue([]);
const readTextFileMock = vi.fn();
const writeTextFileMock = vi.fn().mockResolvedValue(undefined);
const renameMock = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/plugin-fs", () => ({
  BaseDirectory: { AppConfig: "AppConfig" },
  mkdir: (...args: unknown[]) => mkdirMock(...args),
  readDir: (...args: unknown[]) => readDirMock(...args),
  readTextFile: (...args: unknown[]) => readTextFileMock(...args),
  writeTextFile: (...args: unknown[]) => writeTextFileMock(...args),
  rename: (...args: unknown[]) => renameMock(...args),
}));

beforeEach(() => {
  setActivePinia(createPinia());
  mkdirMock.mockClear();
  readDirMock.mockReset().mockResolvedValue([]);
  readTextFileMock.mockReset();
  writeTextFileMock.mockClear();
  renameMock.mockClear();
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

  it("clicking New file resets the store directly when there's nothing to lose", async () => {
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("New configuration"));
    expect(screen.queryByText("There is an unsaved hierarchy in the configuration. What do you want to do?")).toBeNull();
    expect(useHierarchyConfigStore().hierarchy).toHaveLength(0);
  });

  it("clicking New file with an existing hierarchy asks for confirmation first", async () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("New configuration"));
    expect(screen.getByText("There is an unsaved hierarchy in the configuration. What do you want to do?")).toBeTruthy();
    expect(store.hierarchy).toHaveLength(1); // unchanged until the user picks an option
  });

  it("choosing 'Overwrite without saving' resets the store to defaults", async () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.setVmEnabled(true);
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("New configuration"));
    await fireEvent.click(screen.getByText("Overwrite without saving"));
    expect(store.hierarchy).toHaveLength(0);
    expect(store.vmEnabled).toBe(false);
  });

  it("choosing 'Cancel' leaves the hierarchy untouched and closes the modal", async () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    render(HierarchyConfigPanel);
    await fireEvent.click(screen.getByLabelText("New configuration"));
    await fireEvent.click(screen.getByText("Cancel"));
    expect(screen.queryByText("There is an unsaved hierarchy in the configuration. What do you want to do?")).toBeNull();
    expect(store.hierarchy).toHaveLength(1);
  });
});

async function openConfigSelector() {
  await fireEvent.click(screen.getByLabelText("Saved configurations"));
}

describe("HierarchyConfigPanel — saved configs selector + save", () => {
  it("lists saved configs on mount", async () => {
    readDirMock.mockResolvedValue([
      { name: "profile-a.json", isFile: true, isDirectory: false, isSymlink: false },
      { name: "profile-b.json", isFile: true, isDirectory: false, isSymlink: false },
    ]);
    render(HierarchyConfigPanel);
    await Promise.resolve();
    await Promise.resolve();
    await openConfigSelector();
    expect(await screen.findByText("profile-a")).toBeTruthy();
    expect(screen.getByText("profile-b")).toBeTruthy();
  });

  it("selecting a saved config loads it into the store", async () => {
    readDirMock.mockResolvedValue([{ name: "profile-a.json", isFile: true, isDirectory: false, isSymlink: false }]);
    readTextFileMock.mockResolvedValue(JSON.stringify({ hierarchy: [], isa: "RV64I", vmEnabled: true, seed: null, tlb: {}, ram: {}, disk: {} }));

    render(HierarchyConfigPanel);
    await Promise.resolve();
    await Promise.resolve();
    await openConfigSelector();
    await fireEvent.click(await screen.findByText("profile-a"));
    await Promise.resolve();
    await Promise.resolve();

    expect(readTextFileMock).toHaveBeenCalledWith("hierarchy-configs/profile-a.json", { baseDir: "AppConfig" });
    expect(useHierarchyConfigStore().isa).toBe("RV64I");
  });

  it("selecting a saved config while there are unsaved changes asks for confirmation first", async () => {
    readDirMock.mockResolvedValue([{ name: "profile-a.json", isFile: true, isDirectory: false, isSymlink: false }]);
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);

    render(HierarchyConfigPanel);
    await Promise.resolve();
    await Promise.resolve();
    await openConfigSelector();
    await fireEvent.click(await screen.findByText("profile-a"));

    expect(screen.getByText("There is an unsaved hierarchy in the configuration. What do you want to do?")).toBeTruthy();
    expect(readTextFileMock).not.toHaveBeenCalled();
  });

  it("clicking Save with no config selected prompts for a name and creates a new saved entry", async () => {
    render(HierarchyConfigPanel);
    await Promise.resolve();
    await fireEvent.click(screen.getByLabelText("Save configuration"));

    expect(screen.getByText("Save this hierarchy configuration as:")).toBeTruthy();

    await fireEvent.update(screen.getByLabelText("Configuration name"), "my-config");
    await fireEvent.click(screen.getByText("Save"));
    await Promise.resolve();
    await Promise.resolve();

    expect(writeTextFileMock).toHaveBeenCalledWith(
      "hierarchy-configs/my-config.json",
      expect.any(String),
      { baseDir: "AppConfig" },
    );
  });

  it("clicking Save with a config already selected overwrites it in place without prompting", async () => {
    readDirMock.mockResolvedValue([{ name: "profile-a.json", isFile: true, isDirectory: false, isSymlink: false }]);
    readTextFileMock.mockResolvedValue(JSON.stringify({ hierarchy: [], isa: "RV32I", vmEnabled: false, seed: null, tlb: {}, ram: {}, disk: {} }));

    render(HierarchyConfigPanel);
    await Promise.resolve();
    await Promise.resolve();
    await openConfigSelector();
    await fireEvent.click(await screen.findByText("profile-a"));
    await Promise.resolve();
    await Promise.resolve();
    writeTextFileMock.mockClear();

    await fireEvent.click(screen.getByLabelText("Save configuration"));
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.queryByText("Save this hierarchy configuration as:")).toBeNull();
    expect(writeTextFileMock).toHaveBeenCalledWith(
      "hierarchy-configs/profile-a.json",
      expect.any(String),
      { baseDir: "AppConfig" },
    );
  });
});

describe("HierarchyConfigPanel — rename", () => {
  it("disables Rename when no saved config is selected", async () => {
    render(HierarchyConfigPanel);
    await Promise.resolve();
    expect(screen.getByLabelText("Rename configuration")).toBeDisabled();
  });

  it("renames the currently selected config", async () => {
    readDirMock.mockResolvedValue([{ name: "profile-a.json", isFile: true, isDirectory: false, isSymlink: false }]);
    readTextFileMock.mockResolvedValue(JSON.stringify({ hierarchy: [], isa: "RV32I", vmEnabled: false, seed: null, tlb: {}, ram: {}, disk: {} }));

    render(HierarchyConfigPanel);
    await Promise.resolve();
    await Promise.resolve();
    await openConfigSelector();
    await fireEvent.click(await screen.findByText("profile-a"));
    await Promise.resolve();
    await Promise.resolve();

    const renameButton = screen.getByLabelText("Rename configuration");
    expect(renameButton).not.toBeDisabled();
    await fireEvent.click(renameButton);

    expect(screen.getByText("Rename this configuration to:")).toBeTruthy();
    const input = screen.getByLabelText("Configuration name") as HTMLInputElement;
    expect(input.value).toBe("profile-a");

    await fireEvent.update(input, "profile-renamed");
    await fireEvent.click(screen.getByText("Rename"));
    await Promise.resolve();

    expect(renameMock).toHaveBeenCalledWith(
      "hierarchy-configs/profile-a.json",
      "hierarchy-configs/profile-renamed.json",
      { oldPathBaseDir: "AppConfig", newPathBaseDir: "AppConfig" },
    );
  });
});
