import { describe, it, expect } from "vitest";
import { byteSizeLabel } from "./diagram";

describe("byteSizeLabel", () => {
  it("labels bytes below 1024 with no unit scaling", () => {
    expect(byteSizeLabel(8)).toBe("8 B");
  });

  it("scales to KB at 1024 bytes", () => {
    expect(byteSizeLabel(1024)).toBe("1 KB");
  });

  it("scales to MB", () => {
    expect(byteSizeLabel(32 * 1024 * 1024)).toBe("32 MB");
  });

  it("scales to GB", () => {
    expect(byteSizeLabel(4 * 1024 * 1024 * 1024)).toBe("4 GB");
  });

  it("returns 0 B for zero input", () => {
    expect(byteSizeLabel(0)).toBe("0 B");
  });
});
