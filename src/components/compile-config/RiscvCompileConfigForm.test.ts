import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import RiscvCompileConfigForm from "./RiscvCompileConfigForm.vue";
import { useCompileConfigStore } from "../../stores/compileConfig";

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

    // HexField already caps typed input at 8 hex digits, but the setter
    // clamps too as a defense-in-depth measure against u32 overflow.
    await fireEvent.update(dataInput, "FFFFFFFFF");

    expect(useCompileConfigStore().dataBase).toBe(0xffffffff);
  });
});
