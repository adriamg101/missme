import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import RiscvCompileConfigForm from "./RiscvCompileConfigForm.vue";
import { useCompileConfigStore } from "../../stores/compileConfig";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

beforeEach(() => setActivePinia(createPinia()));

describe("RiscvCompileConfigForm", () => {
  it("shows the Memory map heading", () => {
    render(RiscvCompileConfigForm);
    expect(screen.getByText("Memory map")).toBeInTheDocument();
  });

  it("shows .text as a read-only field matching the store's fixed textBase", () => {
    render(RiscvCompileConfigForm);
    const textInput = screen.getByLabelText(".text base address");
    expect(textInput).toHaveValue("00400000");
    expect(textInput).toHaveAttribute("readonly");
  });

  it("shows .data prefilled from the store and updates the store when edited", async () => {
    render(RiscvCompileConfigForm);
    const dataInput = screen.getByLabelText(".data base address");
    expect(dataInput).toHaveValue("10000000");

    await fireEvent.update(dataInput, "20000000");
    expect(useCompileConfigStore().dataBase).toBe(0x20000000);
  });

  it("keeps the previous valid dataBase in the store when the field is cleared, rather than silently treating empty as 0", async () => {
    render(RiscvCompileConfigForm);
    const dataInput = screen.getByLabelText(".data base address");

    await fireEvent.update(dataInput, "20000000");
    expect(useCompileConfigStore().dataBase).toBe(0x20000000);

    await fireEvent.update(dataInput, "");

    expect(useCompileConfigStore().dataBase).toBe(0x20000000);
    expect(dataInput).toHaveValue("");
  });

  it("clamps a .data value beyond u32::MAX to u32::MAX instead of overflowing", async () => {
    render(RiscvCompileConfigForm);
    const dataInput = screen.getByLabelText(".data base address");

    // The hex input field already limits typed input to 8 digits, but the
    // setter clamps the value too, as an extra safety check against overflow.
    await fireEvent.update(dataInput, "FFFFFFFFF");

    expect(useCompileConfigStore().dataBase).toBe(0xffffffff);
  });
});

describe("RiscvCompileConfigForm — .text/.data collision warning", () => {
  it("shows no warning for a .data value with a healthy gap from .text", () => {
    render(RiscvCompileConfigForm);
    expect(screen.queryByText(/too close to \.text/)).toBeNull();
  });

  it("shows a warning when .data is set too close to .text", async () => {
    render(RiscvCompileConfigForm);
    const dataInput = screen.getByLabelText(".data base address");

    await fireEvent.update(dataInput, "00400000");

    expect(screen.getByText(/too close to \.text/)).toBeInTheDocument();
  });

  it("clears the warning once .data is moved back to a safe distance", async () => {
    render(RiscvCompileConfigForm);
    const dataInput = screen.getByLabelText(".data base address");

    await fireEvent.update(dataInput, "00400000");
    expect(screen.getByText(/too close to \.text/)).toBeInTheDocument();

    await fireEvent.update(dataInput, "10000000");
    expect(screen.queryByText(/too close to \.text/)).toBeNull();
  });

  it("uses the exact compiled .text size once available, instead of the heuristic gap", () => {
    const compileStore = useCompileConfigStore();
    compileStore.setCompiledSizes(0x10, 0);
    compileStore.setDataBase(0x00400020); // 32 bytes past the text base, past a 16-byte .text, so no overlap
    render(RiscvCompileConfigForm);
    expect(screen.queryByText(/too close to \.text/)).toBeNull();
  });

  it("warns using the exact compiled .text size when .data is genuinely inside it", () => {
    const compileStore = useCompileConfigStore();
    compileStore.setCompiledSizes(0x100, 0);
    compileStore.setDataBase(0x00400010); // this falls inside a 256-byte .text
    render(RiscvCompileConfigForm);
    expect(screen.getByText(/too close to \.text/)).toBeInTheDocument();
  });
});

describe("RiscvCompileConfigForm — .data fits in configured RAM", () => {
  it("shows no warning when nothing has been compiled yet", () => {
    render(RiscvCompileConfigForm);
    expect(screen.queryByText(/doesn't fit in the configured RAM/)).toBeNull();
  });

  it("shows no warning when the compiled .data segment fits", () => {
    useHierarchyConfigStore().ram = { latency: 100, size: 1024, dirtyPercent: 0 };
    useCompileConfigStore().setCompiledSizes(0, 512);
    render(RiscvCompileConfigForm);
    expect(screen.queryByText(/doesn't fit in the configured RAM/)).toBeNull();
  });

  it("warns when the compiled .data segment exceeds the configured RAM size", () => {
    useHierarchyConfigStore().ram = { latency: 100, size: 1024, dirtyPercent: 0 };
    useCompileConfigStore().setCompiledSizes(0, 2048);
    render(RiscvCompileConfigForm);
    expect(screen.getByText(/doesn't fit in the configured RAM/)).toBeInTheDocument();
  });
});
