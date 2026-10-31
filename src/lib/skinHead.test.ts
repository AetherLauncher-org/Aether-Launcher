import { describe, expect, it, vi } from "vitest";
import { drawHead, headFaces, overlayScale } from "./skinHead";

describe("headFaces", () => {
  const size = 96;
  const [face, hat] = headFaces(size);

  it("has the face and the hat layer, the face first", () => {
    expect(headFaces(size)).toHaveLength(2);
    expect(face.overlay).toBe(false);
    expect(hat.overlay).toBe(true);
  });

  it("reads the front of the head and of the hat layer from the skin", () => {
    expect([face.sx, face.sy]).toEqual([8, 8]);
    expect([hat.sx, hat.sy]).toEqual([40, 8]);
  });

  it("looks straight at the head: no skew, no rotation, the same scale across and down", () => {
    for (const { matrix } of [face, hat]) {
      const [a, b, c, d] = matrix;
      expect(b).toBe(0);
      expect(c).toBe(0);
      expect(a).toBeCloseTo(d, 9);
    }
  });

  it("fills the canvas with the hat layer and centres the face, an eighth smaller", () => {
    expect(hat.matrix).toEqual([size / 8, 0, 0, size / 8, 0, 0]);
    const edge = face.matrix[0] * 8;
    expect(edge).toBeCloseTo(size / overlayScale, 9);
    expect(face.matrix[4]).toBeCloseTo((size - edge) / 2, 9);
    expect(face.matrix[5]).toBeCloseTo((size - edge) / 2, 9);
  });

  it("keeps the colours of the skin", () => {
    expect(face.shade).toBe(0);
    expect(hat.shade).toBe(0);
  });
});

describe("drawHead", () => {
  it("draws the face and then the hat layer, pixel by pixel", () => {
    const context = {
      clearRect: vi.fn(),
      setTransform: vi.fn(),
      drawImage: vi.fn(),
      imageSmoothingEnabled: true,
    };
    const canvas = { width: 96, height: 96, getContext: () => context } as unknown as HTMLCanvasElement;
    const skin = {} as CanvasImageSource;

    expect(drawHead(canvas, skin)).toBe(true);

    expect(context.imageSmoothingEnabled).toBe(false);
    expect(context.drawImage.mock.calls).toEqual([
      [skin, 8, 8, 8, 8, 0, 0, 8, 8],
      [skin, 40, 8, 8, 8, 0, 0, 8, 8],
    ]);
    // the transform is put back
    expect(context.setTransform).toHaveBeenLastCalledWith(1, 0, 0, 1, 0, 0);
  });

  it("says so when there is nothing to draw on", () => {
    const canvas = { width: 96, height: 96, getContext: () => null } as unknown as HTMLCanvasElement;
    expect(drawHead(canvas, {} as CanvasImageSource)).toBe(false);
  });
});
