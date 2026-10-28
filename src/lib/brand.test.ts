import { describe, expect, it } from "vitest";
import { aetherDarkTheme, aetherLightTheme, brandRamp } from "./brand";

function lightness(hex: string): number {
  const channels = [1, 3, 5].map((start) => Number.parseInt(hex.slice(start, start + 2), 16));
  return channels.reduce((sum, value) => sum + value, 0);
}

describe("brand ramp", () => {
  const keys = Array.from({ length: 16 }, (_, index) => (index + 1) * 10) as unknown as Array<
    keyof typeof brandRamp
  >;

  it("has the sixteen Fluent steps, each a six-digit hex colour", () => {
    for (const key of keys) {
      expect(brandRamp[key]).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("gets lighter from step 10 to step 160", () => {
    const values = keys.map((key) => lightness(brandRamp[key]));
    for (let index = 1; index < values.length; index += 1) {
      expect(values[index]).toBeGreaterThan(values[index - 1]);
    }
  });
});

describe("themes", () => {
  it("use the system UI font stack with Segoe UI Variable first", () => {
    expect(aetherLightTheme.fontFamilyBase).toMatch(/^"Segoe UI Variable Text"/);
    expect(aetherDarkTheme.fontFamilyBase).toBe(aetherLightTheme.fontFamilyBase);
  });

  it("take their brand colours from the ramp", () => {
    expect(aetherLightTheme.colorBrandBackground).toBe(brandRamp[80]);
    expect(aetherDarkTheme.colorBrandForeground1).toBe(brandRamp[110]);
  });
});
