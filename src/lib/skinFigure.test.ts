import { describe, expect, it } from "vitest";
import { figureParts, figureSize, isModernSkin, legacyHatIsSolid } from "./skinFigure";

/** 一张 64×32 皮肤的像素：每个点是 `alpha`，要改的点用 `except` 指定 */
function legacyPixels(alpha: number, except: Record<string, number> = {}): Uint8ClampedArray {
  const pixels = new Uint8ClampedArray(64 * 32 * 4);
  for (let y = 0; y < 32; y += 1) {
    for (let x = 0; x < 64; x += 1) {
      pixels[(y * 64 + x) * 4 + 3] = except[`${x},${y}`] ?? alpha;
    }
  }
  return pixels;
}

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

  it("marks exactly the hat part, and it is the one that comes last", () => {
    for (const modern of [true, false]) {
      const parts = figureParts(modern, false);
      expect(parts.filter((part) => part.hat)).toHaveLength(1);
      expect(parts[parts.length - 1].hat).toBe(true);
    }
  });

  it("treats an old skin whose hat area has no transparent pixel as having no hat (Notch's skin)", () => {
    expect(legacyHatIsSolid(legacyPixels(255), 64)).toBe(true);
    // the border of the area counts, and so does a half-transparent pixel
    expect(legacyHatIsSolid(legacyPixels(255, { "32,0": 0 }), 64)).toBe(false);
    expect(legacyHatIsSolid(legacyPixels(255, { "63,31": 127 }), 64)).toBe(false);
    expect(legacyHatIsSolid(legacyPixels(255, { "40,8": 0 }), 64)).toBe(false);
  });

  it("does not let the base head area decide (only x 32-63, y 0-31 is looked at)", () => {
    // a transparent pixel in the base head (x < 32) does not make the hat area count as non-solid
    expect(legacyHatIsSolid(legacyPixels(255, { "10,10": 0 }), 64)).toBe(true);
    // 128 is opaque enough, 127 is not
    expect(legacyHatIsSolid(legacyPixels(128), 64)).toBe(true);
    expect(legacyHatIsSolid(legacyPixels(127), 64)).toBe(false);
  });

  it("mirrors the right arm and leg for an old skin, and has no outer layer but the hat", () => {
    const parts = figureParts(false, false);

    expect(parts.filter((part) => part.flip)).toHaveLength(2);
    expect(parts.filter((part) => part.overlay)).toHaveLength(1);
    expect(parts.every((part) => part.sy < 32 || part.sy === 52 === false)).toBe(true);
  });
});
