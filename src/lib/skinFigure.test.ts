import { describe, expect, it } from "vitest";
import { figureParts, figureSize, isModernSkin } from "./skinFigure";

const inside = (part: { dx: number; dy: number; sw: number; sh: number }) =>
  part.dx >= 0 && part.dy >= 0 && part.dx + part.sw <= figureSize.width && part.dy + part.sh <= figureSize.height;

describe("skin figure", () => {
  it("tells a 64×64 skin from the old 64×32 one", () => {
    expect(isModernSkin(64, 64)).toBe(true);
    expect(isModernSkin(64, 32)).toBe(false);
    expect(isModernSkin(128, 128)).toBe(false);
  });

  it("draws the inner layer first and the outer layer on top, hat last", () => {
    const parts = figureParts(true, false);
    const firstOverlay = parts.findIndex((part) => part.overlay);

    expect(firstOverlay).toBeGreaterThan(0);
    expect(parts.slice(0, firstOverlay).every((part) => !part.overlay)).toBe(true);
    expect(parts.slice(firstOverlay).every((part) => part.overlay)).toBe(true);
    expect(parts[parts.length - 1]).toMatchObject({ sx: 40, sy: 8, overlay: true });
  });

  it("keeps every part inside the 16×32 figure, for both arm widths and both skin formats", () => {
    for (const modern of [true, false]) {
      for (const slim of [true, false]) {
        expect(figureParts(modern, slim).every(inside), `${modern} ${slim}`).toBe(true);
      }
    }
  });

  it("makes slim arms 3 pixels wide and classic arms 4, on the outside of the body", () => {
    const arms = (slim: boolean) => figureParts(true, slim).filter((part) => part.sh === 12 && !part.overlay && (part.sx === 44 || part.sx === 36));

    expect(arms(false).map((part) => part.sw)).toEqual([4, 4]);
    expect(arms(true).map((part) => part.sw)).toEqual([3, 3]);
    // right arm ends where the body starts (x = 4); left arm starts where it ends (x = 12)
    for (const slim of [true, false]) {
      const [right, left] = arms(slim);
      expect(right.dx + right.sw).toBe(4);
      expect(left.dx).toBe(12);
    }
  });

  it("takes the left arm and leg from their own place on a 64×64 skin", () => {
    const parts = figureParts(true, false);

    expect(parts.some((part) => part.sx === 36 && part.sy === 52 && !part.flip)).toBe(true);
    expect(parts.some((part) => part.sx === 20 && part.sy === 52 && !part.flip)).toBe(true);
  });

  it("mirrors the right arm and leg for an old skin, and has no outer layer but the hat", () => {
    const parts = figureParts(false, false);

    expect(parts.filter((part) => part.flip)).toHaveLength(2);
    expect(parts.filter((part) => part.overlay)).toHaveLength(1);
    expect(parts.every((part) => part.sy < 32 || part.sy === 52 === false)).toBe(true);
  });
});
