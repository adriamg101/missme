import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";
import CompileConfigForm from "./CompileConfigForm.vue";

beforeEach(() => setActivePinia(createPinia()));

describe("CompileConfigForm", () => {
  it("renders the RISC-V memory map form when language is RISC-V", () => {
    render(CompileConfigForm, { props: { language: "RISC-V" } });
    expect(screen.getByText("Memory map")).toBeInTheDocument();
  });

  it("renders nothing for an unsupported language", () => {
    const { container } = render(CompileConfigForm, { props: { language: "x86" } });
    expect(container.textContent).toBe("");
  });
});
