import { describe, expect, it } from "vitest";
import { headFaces, overlayScale } from "./skinHead";
import type { HeadFace, Matrix } from "./skinHead";

/** 方块 (u, v)（0–8）贴到画布上的位置。 */
function place([a, b, c, d, e, f]: Matrix, u: number, v: number): [number, number] {
  return [a * u + c * v + e, b * u + d * v + f];
}

function corners(face: HeadFace): [number, number][] {
  return [
    place(face.matrix, 0, 0),
    place(face.matrix, 8, 0),
    place(face.matrix, 0, 8),
    place(face.matrix, 8, 8),
  ];
}

function close(actual: [number, number], expected: [number, number]) {
  expect(actual[0]).toBeCloseTo(expected[0], 6);
  expect(actual[1]).toBeCloseTo(expected[1], 6);
}

describe("headFaces", () => {
  const size = 96;
  const faces = headFaces(size);

  it("has the three visible faces of the head and of the hat layer, the head first", () => {
    expect(faces).toHaveLength(6);
    expect(faces.map((face) => face.overlay)).toEqual([false, false, false, true, true, true]);
  });

  it("reads the skin where the head and the hat layer are", () => {
    expect(faces.map((face) => [face.sx, face.sy])).toEqual([
      [8, 0],
      [8, 8],
      [16, 8],
      [40, 0],
      [40, 8],
      [48, 8],
    ]);
    for (const face of faces) {
      expect(face.sx + 8).toBeLessThanOrEqual(64);
      expect(face.sy + 8).toBeLessThanOrEqual(64);
    }
  });

  it("keeps everything inside the canvas and touches the top and the bottom", () => {
    const points = faces.flatMap(corners);
    const ys = points.map(([, y]) => y);
    const xs = points.map(([x]) => x);
    expect(Math.min(...ys)).toBeCloseTo(0, 6);
    expect(Math.max(...ys)).toBeCloseTo(size, 6);
    expect(Math.min(...xs)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...xs)).toBeLessThanOrEqual(size);
    // centred
    expect((Math.min(...xs) + Math.max(...xs)) / 2).toBeCloseTo(size / 2, 6);
  });

  it("joins the faces along their edges, for the head and for the hat layer", () => {
    for (const first of [0, 3]) {
      const [top, front, side] = faces.slice(first, first + 3);
      // the bottom edge of the top face is the top edge of the front face
      close(place(top.matrix, 0, 8), place(front.matrix, 0, 0));
      close(place(top.matrix, 8, 8), place(front.matrix, 8, 0));
      // the front face's right edge is the side face's left edge
      close(place(front.matrix, 8, 0), place(side.matrix, 0, 0));
      close(place(front.matrix, 8, 8), place(side.matrix, 0, 8));
      // the top face's right edge is the side face's top edge
      close(place(top.matrix, 8, 8), place(side.matrix, 0, 0));
      close(place(top.matrix, 8, 0), place(side.matrix, 8, 0));
    }
  });

  it("makes the hat layer an eighth larger than the head, around the same centre", () => {
    const edge = (face: HeadFace) => {
      const [x0, y0] = place(face.matrix, 0, 0);
      const [x1, y1] = place(face.matrix, 0, 8);
      return Math.hypot(x1 - x0, y1 - y0);
    };
    expect(edge(faces[4]) / edge(faces[1])).toBeCloseTo(overlayScale, 6);
    // the nearest top corner is shared
    close(place(faces[0].matrix, 8, 8), place(faces[3].matrix, 8, 8));
  });

  it("darkens the sides and not the top", () => {
    expect(faces[0].shade).toBe(0);
    expect(faces[1].shade).toBeGreaterThan(0);
    expect(faces[2].shade).toBeGreaterThan(faces[1].shade);
  });
});
