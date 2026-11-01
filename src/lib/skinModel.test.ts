import { describe, expect, it } from "vitest";
import {
  boxFrame,
  boxQuads,
  boxRegion,
  cutoutAlpha,
  faceList,
  faceNormal,
  identity,
  isModernSkin,
  legacyHatIsSolid,
  matMul,
  orthonormalize,
  outerVoxels,
  rotationX,
  rotationY,
  rotationZ,
  skinGroups,
  transform,
  viewFromAngles,
  voxelQuads,
  withOpaqueBase,
} from "./skinModel";
import type { Box } from "./skinModel";

/** 一张 `width`×`height` 皮肤的像素：每个点是 `alpha`，要改的点用 `except`（"x,y" → alpha）指定 */
function pixelsOf(width: number, height: number, alpha: number, except: Record<string, number> = {}): Uint8ClampedArray {
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const index = (y * width + x) * 4;
      pixels[index] = x; // 红色放 x、绿色放 y：看得出取的是哪个像素
      pixels[index + 1] = y;
      pixels[index + 3] = except[`${x},${y}`] ?? alpha;
    }
  }
  return pixels;
}

const head = (): Box => skinGroups(true, false).find((group) => group.base.name === "head")!.base;
const leftArmOfOldSkin = (): Box => skinGroups(false, false).find((group) => group.base.name === "leftArm")!.base;

describe("skin groups", () => {
  it("tells a 64×64 skin from the old 64×32 one", () => {
    expect(isModernSkin(64, 64)).toBe(true);
    expect(isModernSkin(64, 32)).toBe(false);
    expect(isModernSkin(128, 128)).toBe(false);
  });

  it("has the six body parts, every one on its own place, feet on y = 0 and the head ending at y = 32", () => {
    const groups = skinGroups(true, false);

    expect(groups.map((group) => group.base.name)).toEqual(["rightLeg", "leftLeg", "body", "rightArm", "leftArm", "head"]);
    expect(Math.min(...groups.map((group) => group.base.y0))).toBe(0);
    expect(Math.max(...groups.map((group) => group.base.y1))).toBe(32);
    // the character's right arm is on the −x side, and the arms touch the body
    const byName = (name: string) => groups.find((group) => group.base.name === name)!.base;
    expect(byName("rightArm").x1).toBe(byName("body").x0);
    expect(byName("leftArm").x0).toBe(byName("body").x1);
  });

  it("makes slim arms 3 pixels wide and classic arms 4", () => {
    const width = (slim: boolean, name: string) => {
      const box = skinGroups(true, slim).find((group) => group.base.name === name)!.base;
      return box.x1 - box.x0;
    };

    expect(width(false, "rightArm")).toBe(4);
    expect(width(true, "rightArm")).toBe(3);
    expect(width(true, "leftArm")).toBe(3);
    // the slim arm's picture is 3 wide too
    expect(skinGroups(true, true).find((group) => group.base.name === "leftArm")!.base.w).toBe(3);
  });

  it("gives a 64×64 skin all five outer layers and its own left arm and leg", () => {
    const groups = skinGroups(true, false);

    expect(groups.filter((group) => group.outer !== null)).toHaveLength(6);
    expect(groups.some((group) => group.base.mirror)).toBe(false);
    expect(groups.find((group) => group.base.name === "leftArm")!.base).toMatchObject({ u: 32, v: 48 });
    expect(groups.find((group) => group.base.name === "leftLeg")!.base).toMatchObject({ u: 16, v: 48 });
  });

  it("mirrors the right arm and leg for an old skin, and gives it no outer layer but the hat", () => {
    const groups = skinGroups(false, false);

    expect(groups.filter((group) => group.base.mirror).map((group) => group.base.name)).toEqual(["leftLeg", "leftArm"]);
    expect(groups.filter((group) => group.outer !== null).map((group) => group.outer!.name)).toEqual(["hat"]);
    expect(leftArmOfOldSkin()).toMatchObject({ u: 40, v: 16 });
  });

  it("makes the hat thicker than the other outer layers", () => {
    const groups = skinGroups(true, false);
    const hat = groups.find((group) => group.base.name === "head")!;
    const sleeve = groups.find((group) => group.base.name === "rightArm")!;

    expect(hat.thickness).toBeGreaterThan(sleeve.thickness);
  });
});

describe("faces of a box", () => {
  it("lays the faces out as [right][front][left][back] with the top over the front (head at 0,0)", () => {
    const box = head();

    expect(boxRegion(box, "top")).toEqual({ sx: 8, sy: 0, sw: 8, sh: 8 });
    expect(boxRegion(box, "right")).toEqual({ sx: 0, sy: 8, sw: 8, sh: 8 });
    expect(boxRegion(box, "front")).toEqual({ sx: 8, sy: 8, sw: 8, sh: 8 });
    expect(boxRegion(box, "left")).toEqual({ sx: 16, sy: 8, sw: 8, sh: 8 });
    expect(boxRegion(box, "back")).toEqual({ sx: 24, sy: 8, sw: 8, sh: 8 });
  });

  it("uses the 4-deep layout of a body part, not the head's", () => {
    const body = skinGroups(true, false).find((group) => group.base.name === "body")!.base; // 8×12×4 at (16,16)

    expect(boxRegion(body, "front")).toEqual({ sx: 20, sy: 20, sw: 8, sh: 12 });
    expect(boxRegion(body, "right")).toEqual({ sx: 16, sy: 20, sw: 4, sh: 12 });
    expect(boxRegion(body, "left")).toEqual({ sx: 28, sy: 20, sw: 4, sh: 12 });
    expect(boxRegion(body, "back")).toEqual({ sx: 32, sy: 20, sw: 8, sh: 12 });
  });

  it("starts the front picture at its top-left corner as the viewer sees it", () => {
    const frame = boxFrame(head(), "front");

    // x grows to the viewer's right, so the top-left corner is (x0, y1) on the front plane
    expect(frame.origin).toEqual([-4, 32, 4]);
    expect(frame.u).toEqual([1, 0, 0]);
    expect(frame.v).toEqual([0, -1, 0]);
  });

  it("reads the character's right side from the front towards the back… the picture grows towards the front", () => {
    const frame = boxFrame(head(), "right");

    expect(frame.origin).toEqual([-4, 32, -4]);
    expect(frame.u).toEqual([0, 0, 1]);
  });

  it("mirrors an old skin's left arm: the picture runs the other way and the side pictures swap", () => {
    const arm = leftArmOfOldSkin();
    const front = boxFrame(arm, "front");

    // its front picture starts on the far (+x) side and runs towards −x
    expect(front.origin[0]).toBe(arm.x1);
    expect(front.u[0]).toBe(-1);
    // the arm's +x (left) face shows the picture that the right arm has on its −x side
    expect(boxRegion(arm, "left")).toEqual(boxRegion({ ...arm, mirror: false }, "right"));
    expect(boxRegion(arm, "right")).toEqual(boxRegion({ ...arm, mirror: false }, "left"));
    // and that picture still grows towards the front of the character
    expect(boxFrame(arm, "left").u).toEqual([0, 0, 1]);
  });

  it("gives every face an outward normal that is a unit axis", () => {
    for (const face of faceList) {
      const normal = faceNormal(face);
      expect(Math.abs(normal[0]) + Math.abs(normal[1]) + Math.abs(normal[2])).toBe(1);
    }
  });
});

describe("outer layer voxels", () => {
  const hat = (): Box => skinGroups(true, false).find((group) => group.base.name === "head")!.outer!;

  it("makes one voxel per solid pixel of the six faces", () => {
    const voxels = outerVoxels(hat(), 1, pixelsOf(64, 64, 255), 64);

    // top, bottom, front, back, left, right: 8×8 each
    expect(voxels).toHaveLength(6 * 64);
  });

  it("makes none for a transparent pixel and keeps the colour of the pixel it came from", () => {
    // the hat's front is at (40,8); make its first pixel transparent
    const voxels = outerVoxels(hat(), 1, pixelsOf(64, 64, 255, { "40,8": 0 }), 64);

    expect(voxels).toHaveLength(6 * 64 - 1);
    // a front-face voxel: red holds the pixel's x and green its y (see pixelsOf)
    const front = voxels.find((voxel) => voxel.color[0] === 41 && voxel.color[1] === 8)!;
    expect(front.walls[0]).toEqual([2, 1]);
  });

  it("sticks the voxels out of the base by the thickness", () => {
    const voxels = outerVoxels(hat(), 1, pixelsOf(64, 64, 255), 64);
    const front = voxels.find((voxel) => voxel.color[0] === 43 && voxel.color[1] === 11)!; // a middle pixel

    expect(front.min[2]).toBe(4);
    expect(front.max[2]).toBe(5);
    expect(front.max[0] - front.min[0]).toBe(1);
    expect(front.max[1] - front.min[1]).toBe(1);
  });

  it("reaches over the edge of the face, so voxels of two faces meet on the box edge", () => {
    const voxels = outerVoxels(hat(), 1, pixelsOf(64, 64, 255), 64);
    const corner = voxels.find((voxel) => voxel.color[0] === 40 && voxel.color[1] === 8)!; // front's top-left pixel

    expect(corner.min[0]).toBe(-5); // one past the box's x0 = −4
    expect(corner.max[1]).toBe(33); // one past y1 = 32
  });

  it("only exposes a side wall where the neighbour is transparent or the face ends", () => {
    // a lone opaque pixel in the middle of the front face
    const pixels = pixelsOf(64, 64, 0, { "43,11": 255 });
    const voxels = outerVoxels(hat(), 1, pixels, 64);

    expect(voxels).toHaveLength(1);
    expect(voxels[0].walls).toHaveLength(5); // outward + four sides

    // two neighbours side by side hide the wall between them
    const pair = outerVoxels(hat(), 1, pixelsOf(64, 64, 0, { "43,11": 255, "44,11": 255 }), 64);
    expect(pair).toHaveLength(2);
    expect(pair.every((voxel) => voxel.walls.length === 4)).toBe(true);
  });

  it("builds a mirrored limb from the picture of the right one", () => {
    const sleeveOfOldSkin = { ...leftArmOfOldSkin(), v: 32 }; // pretend an outer layer exists
    const voxels = outerVoxels(sleeveOfOldSkin, 0.5, pixelsOf(64, 64, 255), 64);

    expect(voxels.length).toBeGreaterThan(0);
    // the front picture of a mirrored arm is read from the right arm's front: x 44..47
    const frontReds = new Set(voxels.filter((voxel) => voxel.walls[0][0] === 2 && voxel.walls[0][1] === 1).map((v) => v.color[0]));
    expect([...frontReds].sort((a, b) => a - b)).toEqual([44, 45, 46, 47]);
  });
});

describe("rotation", () => {
  const near = (a: readonly number[], b: readonly number[]) => a.forEach((value, index) => expect(value).toBeCloseTo(b[index]));

  it("leaves the front facing the viewer with no rotation and shows no top at pitch 0", () => {
    expect(transform(identity, faceNormal("front"))).toEqual([0, 0, 1]);
    expect(transform(viewFromAngles(0, 0), faceNormal("top"))[2]).toBeCloseTo(0);
  });

  it("shows the top when looking down, and the character's right side when it turns to its right", () => {
    expect(transform(viewFromAngles(0, 0.3), faceNormal("top"))[2]).toBeGreaterThan(0);
    expect(transform(viewFromAngles(0.5, 0), faceNormal("right"))[2]).toBeGreaterThan(0);
    expect(transform(viewFromAngles(0.5, 0), faceNormal("left"))[2]).toBeLessThan(0);
    expect(transform(viewFromAngles(-0.5, 0), faceNormal("left"))[2]).toBeGreaterThan(0);
  });

  it("turns the back to the viewer at half a turn, and the bottom when tipped over", () => {
    expect(transform(viewFromAngles(Math.PI, 0), faceNormal("back"))[2]).toBeCloseTo(1);
    expect(transform(viewFromAngles(0, -0.5), faceNormal("bottom"))[2]).toBeGreaterThan(0);
    expect(transform(rotationX(Math.PI), faceNormal("bottom"))[2]).toBeCloseTo(0);
  });

  it("rolls in the screen plane around the axis that points at the viewer", () => {
    // a quarter turn of roll sends the right of the screen to the top
    near(transform(rotationZ(Math.PI / 2), [1, 0, 0]), [0, 1, 0]);
    near(transform(rotationZ(Math.PI / 2), [0, 0, 1]), [0, 0, 1]);
  });

  it("composes: a turn about one axis then another differs from the other order", () => {
    const a = matMul(rotationX(0.4), rotationY(0.7));
    const b = matMul(rotationY(0.7), rotationX(0.4));

    expect(a.some((value, index) => Math.abs(value - b[index]) > 1e-6)).toBe(true);
    near(matMul(identity, a), a);
  });

  it("keeps a matrix a pure rotation after many small turns", () => {
    let m = viewFromAngles(-0.5, 0.2);
    for (let step = 0; step < 5000; step += 1) {
      m = orthonormalize(matMul(matMul(rotationX(0.013), rotationY(-0.021)), m));
    }
    // rows are unit length and perpendicular
    const row = (r: number) => [m[r * 3], m[r * 3 + 1], m[r * 3 + 2]] as const;
    const dot = (p: readonly number[], q: readonly number[]) => p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    expect(dot(row(0), row(0))).toBeCloseTo(1, 9);
    expect(dot(row(1), row(1))).toBeCloseTo(1, 9);
    expect(dot(row(2), row(2))).toBeCloseTo(1, 9);
    expect(dot(row(0), row(1))).toBeCloseTo(0, 9);
    expect(dot(row(1), row(2))).toBeCloseTo(0, 9);
  });
});

describe("the game's cutout rule for skins", () => {
  const hat = (): Box => skinGroups(true, false).find((group) => group.base.name === "head")!.outer!;

  it("drops pixels below alpha 26 and draws the rest fully opaque, whatever their alpha", () => {
    // a lone front-face pixel of the hat at (43,11)
    expect(outerVoxels(hat(), 1, pixelsOf(64, 64, 0, { "43,11": cutoutAlpha - 1 }), 64)).toHaveLength(0);
    const kept = outerVoxels(hat(), 1, pixelsOf(64, 64, 0, { "43,11": cutoutAlpha }), 64);
    expect(kept).toHaveLength(1);
    expect(kept[0].color).toHaveLength(3); // colour only: there is no alpha left to blend with
    expect(outerVoxels(hat(), 1, pixelsOf(64, 64, 0, { "43,11": 100 }), 64)).toHaveLength(1);
  });

  it("makes the inner layer opaque and leaves the outer layer's alpha alone", () => {
    const pixels = pixelsOf(64, 64, 0);
    const opaque = withOpaqueBase(pixels, 64, 64);

    // head base (0..31, 0..15), body/right limbs (0..63, 16..31), left limbs (16..47, 48..63)
    expect(opaque[(5 * 64 + 5) * 4 + 3]).toBe(255);
    expect(opaque[(20 * 64 + 60) * 4 + 3]).toBe(255);
    expect(opaque[(50 * 64 + 20) * 4 + 3]).toBe(255);
    // the hat, the jacket and the left pants/sleeve stay as they were
    expect(opaque[(5 * 64 + 40) * 4 + 3]).toBe(0);
    expect(opaque[(40 * 64 + 20) * 4 + 3]).toBe(0);
    expect(opaque[(60 * 64 + 5) * 4 + 3]).toBe(0);
    // and the input is not changed
    expect(pixels[(5 * 64 + 5) * 4 + 3]).toBe(0);
  });

  it("copes with the old 64×32 size", () => {
    const opaque = withOpaqueBase(pixelsOf(64, 32, 0), 64, 32);

    expect(opaque[(20 * 64 + 20) * 4 + 3]).toBe(255);
    expect(opaque.length).toBe(64 * 32 * 4);
  });

  it("gives every solid inner pixel a quad, six faces of a head: 6 × 64", () => {
    const head = skinGroups(true, false).find((group) => group.base.name === "head")!.base;
    const quads = boxQuads(head, withOpaqueBase(pixelsOf(64, 64, 0), 64, 64), 64);

    expect(quads).toHaveLength(6 * 64);
  });
});

describe("faces: the bottom", () => {
  it("is next to the top in the layout and runs front to back", () => {
    const head = skinGroups(true, false).find((group) => group.base.name === "head")!.base;

    expect(boxRegion(head, "bottom")).toEqual({ sx: 16, sy: 0, sw: 8, sh: 8 });
    expect(boxFrame(head, "bottom").origin).toEqual([-4, 24, 4]);
    expect(boxFrame(head, "bottom").v).toEqual([0, 0, -1]);
  });
});

describe("voxel quads", () => {
  it("makes one quad per exposed wall, facing outward with the wall's normal", () => {
    const hat = skinGroups(true, false).find((group) => group.base.name === "head")!.outer!;
    const [voxel] = outerVoxels(hat, 1, pixelsOf(64, 64, 0, { "43,11": 255 }), 64);
    const quads = voxelQuads(voxel);

    expect(quads).toHaveLength(voxel.walls.length);
    const front = quads.find((quad) => quad.normal[2] === 1)!;
    // the front wall lies on the plane z = 5 (the box's z1 = 4 plus the thickness 1)
    expect(front.corners.every((corner) => corner[2] === 5)).toBe(true);
    expect(front.color).toEqual(voxel.color);
  });
});

describe("the old-skin hat rule (Notch's skin)", () => {
  it("treats a hat area with no transparent pixel as having no hat", () => {
    expect(legacyHatIsSolid(pixelsOf(64, 32, 255), 64)).toBe(true);
  });

  it("counts the corners of the area, and a half-transparent pixel", () => {
    expect(legacyHatIsSolid(pixelsOf(64, 32, 255, { "32,0": 0 }), 64)).toBe(false);
    expect(legacyHatIsSolid(pixelsOf(64, 32, 255, { "63,31": 127 }), 64)).toBe(false);
    expect(legacyHatIsSolid(pixelsOf(64, 32, 255, { "40,8": 0 }), 64)).toBe(false);
  });

  it("only looks at x 32–63, y 0–31, and 128 is opaque enough where 127 is not", () => {
    expect(legacyHatIsSolid(pixelsOf(64, 32, 255, { "10,10": 0 }), 64)).toBe(true);
    expect(legacyHatIsSolid(pixelsOf(64, 32, 128), 64)).toBe(true);
    expect(legacyHatIsSolid(pixelsOf(64, 32, 127), 64)).toBe(false);
  });
});
