import { describe, expect, it } from "vitest";
import { identity, viewFromAngles } from "./skinModel";
import type { Quad } from "./skinModel";
import { prepareFromPixels, rasterize, viewSize } from "./skinRender";

/** 一张 64×64 的皮肤：每个像素用 `paint(x, y)` 给的颜色，没给就是全透明。 */
function skinPixels(paint: (x: number, y: number) => readonly [number, number, number, number] | null): Uint8ClampedArray {
  const pixels = new Uint8ClampedArray(64 * 64 * 4);
  for (let y = 0; y < 64; y += 1) {
    for (let x = 0; x < 64; x += 1) {
      const color = paint(x, y);
      if (color !== null) {
        pixels.set(color, (y * 64 + x) * 4);
      }
    }
  }
  return pixels;
}

const W = 220;
const H = 360;
const colourAt = (frame: { pixels: Uint8ClampedArray }, x: number, y: number) => {
  const index = (y * W + x) * 4;
  return [frame.pixels[index], frame.pixels[index + 1], frame.pixels[index + 2], frame.pixels[index + 3]];
};

/** 一个朝向观察者的正方形，边长 `size`，中心在 (cx, cy)（皮肤像素），深度 `z`，纯色。 */
function square(cx: number, cy: number, z: number, size: number, color: readonly [number, number, number]): Quad {
  const h = size / 2;
  return {
    corners: [
      [cx - h, cy + h, z],
      [cx + h, cy + h, z],
      [cx + h, cy - h, z],
      [cx - h, cy - h, z],
    ],
    color,
    normal: [0, 0, 1],
  };
}

describe("rasterize", () => {
  it("draws what is nearer on top, whatever order the quads come in", () => {
    const far = square(0, 16, -3, 8, [255, 0, 0]);
    const near = square(0, 16, 3, 4, [0, 255, 0]);
    const centre = [W / 2, H / 2] as const;

    for (const quads of [[far, near], [near, far]]) {
      const frame = rasterize({ quads }, identity, W, H);
      const [r, g] = colourAt(frame, centre[0], centre[1]);
      expect(g, "the nearer green square covers the centre").toBeGreaterThan(r);
    }
    // outside the small square but inside the big one, the far one shows
    const frame = rasterize({ quads: [near, far] }, identity, W, H);
    const scale = W / viewSize.width;
    const [r, g] = colourAt(frame, Math.round(W / 2 + 3 * scale), H / 2);
    expect(r).toBeGreaterThan(g);
  });

  it("leaves out a quad that faces away from the viewer", () => {
    const back: Quad = { ...square(0, 16, 0, 8, [255, 0, 0]), normal: [0, 0, -1] };
    const frame = rasterize({ quads: [back] }, identity, W, H);

    expect(colourAt(frame, W / 2, H / 2)[3]).toBe(0);
  });

  it("covers a quad completely, with no cracks between two quads that share an edge", () => {
    const left = square(-2, 16, 0, 4, [200, 200, 200]);
    const right = square(2, 16, 0, 4, [200, 200, 200]);
    const frame = rasterize({ quads: [left, right] }, identity, W, H);
    const scale = W / viewSize.width;
    // every pixel across the seam and inside the pair is covered
    for (let x = Math.round(W / 2 - 3.9 * scale); x <= Math.round(W / 2 + 3.9 * scale); x += 1) {
      expect(colourAt(frame, x, H / 2)[3], `x = ${x}`).toBe(255);
    }
  });

  it("keeps the figure inside the picture from any direction", () => {
    const pixels = skinPixels(() => [120, 120, 120, 255]);
    const skin = prepareFromPixels(pixels, 64, 64, false);

    for (const [yaw, pitch, roll] of [[0, 0, 0], [1, 0.5, 0], [3, -1, 0.4], [0.3, 1.4, 2], [-2, 0.1, -1]]) {
      const frame = rasterize(skin, viewFromAngles(yaw, pitch, roll), W, H);
      const edge: number[] = [];
      for (let x = 0; x < W; x += 1) {
        edge.push(frame.pixels[x * 4 + 3], frame.pixels[((H - 1) * W + x) * 4 + 3]);
      }
      for (let y = 0; y < H; y += 1) {
        edge.push(frame.pixels[y * W * 4 + 3], frame.pixels[(y * W + W - 1) * 4 + 3]);
      }
      expect(Math.max(...edge), `${yaw} ${pitch} ${roll}`).toBe(0);
      expect(frame.depth.some((d) => d > Number.NEGATIVE_INFINITY)).toBe(true);
    }
  });
});

describe("a skin on screen: no seeing through", () => {
  // the body (inner layer) is red; the jacket (outer layer) is green. From the front the jacket covers the body.
  const front = viewFromAngles(0, 0);
  const bodyFront = (alpha: number) =>
    skinPixels((x, y) => {
      if (x >= 20 && x < 28 && y >= 20 && y < 32) {
        return [255, 0, 0, 255]; // body, front
      }
      if (x >= 20 && x < 28 && y >= 36 && y < 48) {
        return [0, 255, 0, alpha]; // jacket, front
      }
      return null;
    });
  const bodyCentre = (frame: { pixels: Uint8ClampedArray }) => colourAt(frame, W / 2, Math.round(H / 2 - 0 * 1));

  it("draws an opaque outer layer over the inner one", () => {
    const frame = rasterize(prepareFromPixels(bodyFront(255), 64, 64, false), front, W, H);
    const [r, g] = bodyCentre(frame);

    expect(g).toBeGreaterThan(r);
  });

  it("draws a half-transparent outer pixel fully opaque, as the game does, and not blended with the body", () => {
    const frame = rasterize(prepareFromPixels(bodyFront(100), 64, 64, false), front, W, H);
    const [r, g, , a] = bodyCentre(frame);

    expect(a).toBe(255);
    expect(g).toBeGreaterThan(r + 100); // clearly the jacket's green, with no red mixed in from below
  });

  it("drops an almost transparent outer pixel, so the body shows", () => {
    const frame = rasterize(prepareFromPixels(bodyFront(20), 64, 64, false), front, W, H);
    const [r, g] = bodyCentre(frame);

    expect(r).toBeGreaterThan(g);
  });

  it("has no holes in the inner layer even where the picture is transparent", () => {
    // an all-transparent skin: the inner layer is still drawn, in black, because the game forces it opaque
    const frame = rasterize(prepareFromPixels(skinPixels(() => null), 64, 64, false), front, W, H);

    expect(bodyCentre(frame)[3]).toBe(255);
  });

  it("skips the hat of an old skin whose hat area is solid (Notch), and draws it when it is not", () => {
    const old = (solid: boolean) => {
      const pixels = new Uint8ClampedArray(64 * 32 * 4);
      for (let y = 0; y < 32; y += 1) {
        for (let x = 0; x < 64; x += 1) {
          const inHat = x >= 40 && x < 48 && y >= 8 && y < 16; // the hat's front
          const inHatArea = x >= 32;
          const alpha = inHatArea ? (solid || inHat ? 255 : 0) : 255;
          // the face (8..16, 8..16) is skin-coloured; the hat front is black
          pixels.set(inHat ? [0, 0, 0, alpha] : [238, 180, 159, alpha], (y * 64 + x) * 4);
        }
      }
      return pixels;
    };
    // look at the middle of the head from the front
    const headCentre = (frame: { pixels: Uint8ClampedArray }) => {
      const scale = W / viewSize.width;
      return colourAt(frame, W / 2, Math.round(H / 2 - 12 * scale));
    };

    const solid = rasterize(prepareFromPixels(old(true), 64, 32, false), front, W, H);
    expect(headCentre(solid)[0], "face colour, not black").toBeGreaterThan(100);

    const withHat = rasterize(prepareFromPixels(old(false), 64, 32, false), front, W, H);
    expect(headCentre(withHat)[0], "a real hat is drawn over the face").toBeLessThan(40);
  });
});
