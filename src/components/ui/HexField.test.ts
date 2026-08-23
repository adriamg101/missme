import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import HexField from "./HexField.vue";

describe("HexField", () => {
  it("renders the 0x prefix and the label", () => {
    render(HexField, { props: { modelValue: "10000000", label: ".data", ariaLabel: ".data base address" } });
    expect(screen.getByText("0x")).toBeInTheDocument();
    expect(screen.getByText(".data")).toBeInTheDocument();
  });

  it("strips non-hex characters and uppercases the emitted value", async () => {
    const { emitted } = render(HexField, { props: { modelValue: "", ariaLabel: "field" } });
    await fireEvent.update(screen.getByLabelText("field"), "1a2g3B");
    expect(emitted("update:modelValue")?.[0]).toEqual(["1A23B"]);
  });

  it("marks the input readonly when the readonly prop is set", () => {
    render(HexField, { props: { modelValue: "00400000", ariaLabel: "field", readonly: true } });
    expect(screen.getByLabelText("field")).toHaveAttribute("readonly");
  });

  it("caps input at 8 hex digits by default (does not exceed u32 range)", async () => {
    const { emitted } = render(HexField, { props: { modelValue: "", ariaLabel: "field" } });
    const input = screen.getByLabelText("field") as HTMLInputElement;

    await fireEvent.update(input, "FFFFFFFFF"); // 9 hex digits

    expect(emitted("update:modelValue")?.[0]).toEqual(["FFFFFFFF"]);
    expect(input.value).toBe("FFFFFFFF");
  });

  it("sets the maxlength attribute on the underlying input", () => {
    render(HexField, { props: { modelValue: "", ariaLabel: "field" } });
    expect(screen.getByLabelText("field")).toHaveAttribute("maxlength", "8");
  });

  it("respects a custom maxLength prop", async () => {
    const { emitted } = render(HexField, { props: { modelValue: "", ariaLabel: "field", maxLength: 4 } });
    const input = screen.getByLabelText("field") as HTMLInputElement;

    await fireEvent.update(input, "ABCDE");

    expect(emitted("update:modelValue")?.[0]).toEqual(["ABCD"]);
    expect(input.value).toBe("ABCD");
  });

  it("corrects the visible DOM value when sanitized input round-trips to the same modelValue prop", async () => {
    // Regression for the controlled-input desync: when the sanitized
    // string equals the modelValue the prop already held, Vue's :value
    // binding never rewrites the DOM (no prop change), so the raw text
    // the user typed could otherwise stay stuck on screen.
    const { rerender } = render(HexField, { props: { modelValue: "1A2B3C4D", ariaLabel: "field" } });
    const input = screen.getByLabelText("field") as HTMLInputElement;

    // Simulate typing an invalid trailing character; the sanitized value
    // is identical to the modelValue prop already bound.
    await fireEvent.update(input, "1A2B3C4Dz");

    expect(input.value).toBe("1A2B3C4D");

    // Even if the parent doesn't change modelValue (because the emitted
    // value already matches it), the DOM must not keep showing the raw,
    // un-sanitized text.
    await rerender({ modelValue: "1A2B3C4D" });
    expect(input.value).toBe("1A2B3C4D");
  });
});
