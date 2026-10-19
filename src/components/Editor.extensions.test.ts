import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import Editor from "./Editor.vue";

beforeEach(() => {
  setActivePinia(createPinia());
});

async function openOptionsMenu() {
  render(Editor, { global: { plugins: [] } });
  await fireEvent.click(screen.getByLabelText("Compilation options"));
}

function checkbox(extension: "M" | "F" | "D" | "Q"): HTMLInputElement {
  return screen.getByLabelText(extension) as HTMLInputElement;
}

describe("Editor extension checkboxes", () => {
  it("checking D also checks F", async () => {
    await openOptionsMenu();
    await fireEvent.click(checkbox("D"));
    expect(checkbox("D").checked).toBe(true);
    expect(checkbox("F").checked).toBe(true);
  });

  it("unchecking F also unchecks D", async () => {
    await openOptionsMenu();
    await fireEvent.click(checkbox("D"));
    await fireEvent.click(checkbox("F"));
    expect(checkbox("F").checked).toBe(false);
    expect(checkbox("D").checked).toBe(false);
  });

  it("checking Q also checks D and F", async () => {
    await openOptionsMenu();
    await fireEvent.click(checkbox("Q"));
    expect(checkbox("Q").checked).toBe(true);
    expect(checkbox("D").checked).toBe(true);
    expect(checkbox("F").checked).toBe(true);
  });

  it("unchecking F also unchecks D and Q", async () => {
    await openOptionsMenu();
    await fireEvent.click(checkbox("Q"));
    await fireEvent.click(checkbox("F"));
    expect(checkbox("F").checked).toBe(false);
    expect(checkbox("D").checked).toBe(false);
    expect(checkbox("Q").checked).toBe(false);
  });

  it("unchecking D also unchecks Q, but leaves F checked", async () => {
    await openOptionsMenu();
    await fireEvent.click(checkbox("Q"));
    await fireEvent.click(checkbox("D"));
    expect(checkbox("D").checked).toBe(false);
    expect(checkbox("Q").checked).toBe(false);
    expect(checkbox("F").checked).toBe(true);
  });

  it("M is unaffected by the F/D/Q cascade", async () => {
    await openOptionsMenu();
    await fireEvent.click(checkbox("M"));
    await fireEvent.click(checkbox("Q"));
    expect(checkbox("M").checked).toBe(true);
    await fireEvent.click(checkbox("F"));
    expect(checkbox("M").checked).toBe(true);
  });
});
