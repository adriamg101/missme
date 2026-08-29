import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import { mount } from "@vue/test-utils";

const invokeMock = vi.fn().mockResolvedValue({ error_count: 0, error_output: "", config: {} });
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

import Editor from "./Editor.vue";
import { useCompileConfigStore } from "../stores/compileConfig";

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockClear();
});

describe("Editor compile configuration", () => {
  it("renders the Memory map form", () => {
    render(Editor);
    expect(screen.getByText("Memory map")).toBeInTheDocument();
  });

  it("sends the store's text_base/data_base as codegenConfig (Tauri's expected camelCase key) when compiling", async () => {
    render(Editor);
    useCompileConfigStore().setDataBase(0x20000000);

    await fireEvent.click(screen.getByLabelText("Compile"));

    expect(invokeMock).toHaveBeenCalledWith("compile", {
      source: "",
      config: { base: "RV32I", extensions: [] },
      codegenConfig: { text_base: 0x00400000, data_base: 0x20000000 },
    });
  });

  it("does not render an empty bordered compile-config strip for a language with no compile-config form", async () => {
    const wrapper = mount(Editor, {
      global: { plugins: [createPinia()] },
    });

    // Today `languages` only contains "RISC-V", so there's no UI path to
    // reach this state yet — simulate the day a second, unsupported
    // language is added by forcing selectedLanguage directly.
    (wrapper.vm as unknown as { selectedLanguage: string }).selectedLanguage = "x86";
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".editor-compile-config").exists()).toBe(false);
  });
});
