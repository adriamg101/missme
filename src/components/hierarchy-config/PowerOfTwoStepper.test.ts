import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";

describe("PowerOfTwoStepper", () => {
  it("doubles the value on the increment button", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    await fireEvent.click(screen.getByLabelText("Increment"));
    expect(emitted()["update:modelValue"][0]).toEqual([128]);
  });

  it("halves the value on the decrement button", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    await fireEvent.click(screen.getByLabelText("Decrement"));
    expect(emitted()["update:modelValue"][0]).toEqual([32]);
  });

  it("does not go below min", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 1, min: 1 } });
    await fireEvent.click(screen.getByLabelText("Decrement"));
    expect(emitted()["update:modelValue"]).toBeUndefined();
  });

  it("does not go above max", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 128, max: 128 } });
    await fireEvent.click(screen.getByLabelText("Increment"));
    expect(emitted()["update:modelValue"]).toBeUndefined();
  });

  it("shows the current value formatted with its unit, and the input is not readonly", () => {
    render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.readOnly).toBe(false);
    expect(input.value).toBe("64");
  });

  it("typing a plain number rounds it to the nearest power of two on blur", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    await fireEvent.update(input, "1023");
    await fireEvent.blur(input);
    expect(emitted()["update:modelValue"][0]).toEqual([1024]);
    expect(input.value).toBe("1 K");
  });

  it("accepts a unit suffix, case-insensitively and with or without a space", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    await fireEvent.update(input, "2 m");
    await fireEvent.blur(input);
    expect(emitted()["update:modelValue"][0]).toEqual([2 * 1024 ** 2]);
  });

  it("reverts to the previous value and does not emit when the unit letter is invalid", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 1024 } });
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    await fireEvent.update(input, "8B");
    await fireEvent.blur(input);
    expect(emitted()["update:modelValue"]).toBeUndefined();
    expect(input.value).toBe("1 K");
  });

  it("flashes the input invalid on bad input, and clears the flash after it settles", async () => {
    vi.useFakeTimers();
    try {
      render(PowerOfTwoStepper, { props: { modelValue: 1024 } });
      const input = screen.getByRole("spinbutton") as HTMLInputElement;
      await fireEvent.update(input, "not-a-size");
      await fireEvent.blur(input);
      expect(input.className).toContain("power-of-two-stepper-input-invalid");
      await vi.advanceTimersByTimeAsync(500);
      expect(input.className).not.toContain("power-of-two-stepper-input-invalid");
    } finally {
      vi.useRealTimers();
    }
  });

  it("clamps a valid rounded value to max", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 64, max: 128 } });
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    await fireEvent.update(input, "4K");
    await fireEvent.blur(input);
    expect(emitted()["update:modelValue"][0]).toEqual([128]);
  });
});
