import { describe, expect, it } from "vitest";
import { applyLook, defaultSpeed, lookDirection, noKeys, startPose, stepFly } from "./camera";
import { blockLook, cssColor, displayBlockName, shortId } from "./colors";
import { decodeRuns } from "./decode";
import { VoxelGrid } from "./grid";
import { CHUNK, chunkCounts, meshChunk } from "./mesher";
import { pick } from "./picker";

const AIR = { name: "minecraft:air" };
const STONE = { name: "minecraft:stone" };
const GLASS = { name: "minecraft:glass" };
const GOLD = { name: "minecraft:gold_block" };

/** 用一个函数描述方块：返回调色板序号。 */
function grid(size: [number, number, number], palette: { name: string }[], at: (x: number, y: number, z: number) => number) {
  const [sx, sy, sz] = size;
  const blocks = new Uint16Array(sx * sy * sz);
  for (let y = 0; y < sy; y += 1) {
    for (let z = 0; z < sz; z += 1) {
      for (let x = 0; x < sx; x += 1) {
        blocks[(y * sz + z) * sx + x] = at(x, y, z);
      }
    }
  }
  return new VoxelGrid(sx, sy, sz, blocks, palette);
}

function toBase64(bytes: number[]): string {
  return btoa(String.fromCharCode(...bytes));
}

function runBytes(runs: [number, number][]): number[] {
  return runs.flatMap(([value, count]) => [value & 255, value >> 8, 0, 0, count & 255, (count >> 8) & 255, 0, 0]);
}

describe("decodeRuns", () => {
  it("expands value and count pairs into one entry per block", () => {
    const base64 = toBase64(runBytes([[0, 3], [5, 2], [0, 1]]));
    expect(Array.from(decodeRuns(base64, 3, 6))).toEqual([0, 0, 0, 5, 5, 0]);
  });

  it("refuses data that does not add up", () => {
    const ok = toBase64(runBytes([[1, 4]]));
    expect(() => decodeRuns(ok, 1, 5)).toThrow();
    expect(() => decodeRuns(ok, 1, 3)).toThrow();
    expect(() => decodeRuns(ok, 2, 4)).toThrow();
    expect(() => decodeRuns(toBase64(runBytes([[1, 0]])), 1, 0)).toThrow();
    expect(() => decodeRuns(toBase64([1, 0, 1, 0, 4, 0, 0, 0]), 1, 4)).toThrow(); // value above u16
  });
});

describe("blockLook", () => {
  it("does not draw air and cannot point at it", () => {
    for (const name of ["minecraft:air", "minecraft:cave_air", "minecraft:void_air"]) {
      const look = blockLook(name);
      expect(look.draw).toBe(false);
      expect(look.pick).toBe(false);
    }
  });

  it("draws light and barrier blocks as nothing but still lets you point at them", () => {
    expect(blockLook("minecraft:barrier")).toMatchObject({ draw: false, pick: true });
  });

  it("makes glass, ice and water see-through, and stone solid", () => {
    for (const name of ["minecraft:glass", "minecraft:blue_stained_glass", "minecraft:glass_pane", "minecraft:ice", "minecraft:water"]) {
      expect(blockLook(name)).toMatchObject({ translucent: true, occlude: false, draw: true });
    }
    expect(blockLook("minecraft:stone")).toMatchObject({ translucent: false, occlude: true });
    expect(blockLook("minecraft:blue_ice").translucent).toBe(false);
  });

  it("gives the same block the same colour, and different blocks different ones", () => {
    expect(blockLook("minecraft:some_modded_thing")).toEqual(blockLook("minecraft:some_modded_thing"));
    expect(blockLook("create:brass_casing").rgb).not.toEqual(blockLook("create:andesite_casing").rgb);
  });

  it("knows dyed blocks, woods and stripped logs", () => {
    expect(cssColor(blockLook("minecraft:red_concrete").rgb)).toBe("#b02e26");
    expect(blockLook("minecraft:red_terracotta").rgb[0]).toBeLessThan(blockLook("minecraft:red_concrete").rgb[0]);
    expect(blockLook("minecraft:oak_stairs").rgb).toEqual(blockLook("minecraft:oak_planks").rgb);
    expect(blockLook("minecraft:stripped_oak_log").rgb[0]).toBeGreaterThan(blockLook("minecraft:oak_log").rgb[0]);
    expect(blockLook("minecraft:dark_oak_planks").rgb).not.toEqual(blockLook("minecraft:oak_planks").rgb);
  });

  it("names blocks for people", () => {
    expect(shortId("minecraft:oak_planks")).toBe("oak_planks");
    expect(displayBlockName("minecraft:oak_planks")).toBe("Oak Planks");
    expect(displayBlockName("create:brass_casing")).toBe("Brass Casing (create)");
  });
});

describe("meshChunk", () => {
  const palette = [AIR, STONE, GLASS, GOLD];

  it("gives a lone cube six faces", () => {
    const g = grid([1, 1, 1], palette, () => 1);
    const mesh = meshChunk(g, 0, 0, 0);
    expect(mesh.quads).toBe(6);
    expect(mesh.opaque?.indices.length).toBe(36);
    expect(mesh.opaque?.positions.length).toBe(24 * 3);
    expect(mesh.translucent).toBeNull();
  });

  it("returns nothing for an empty chunk", () => {
    const mesh = meshChunk(grid([4, 4, 4], palette, () => 0), 0, 0, 0);
    expect(mesh.quads).toBe(0);
    expect(mesh.opaque).toBeNull();
  });

  it("merges a solid block of one kind into one quad per side, however big", () => {
    const g = grid([CHUNK, CHUNK, CHUNK], palette, () => 1);
    expect(meshChunk(g, 0, 0, 0).quads).toBe(6);
  });

  it("hides the faces that touch, and merges a row into long quads", () => {
    const g = grid([8, 1, 1], palette, () => 1);
    // 8 cubes in a row: two end caps, and top/bottom/front/back each one long quad.
    expect(meshChunk(g, 0, 0, 0).quads).toBe(6);
  });

  it("does not merge different blocks", () => {
    const g = grid([2, 1, 1], palette, (x) => (x === 0 ? 1 : 3));
    // Stone and gold touching: the two end faces and 4 sides each = 5 + 5.
    expect(meshChunk(g, 0, 0, 0).quads).toBe(10);
  });

  it("draws the faces of an opaque block that touch glass, but not glass against glass", () => {
    const stoneAndGlass = grid([2, 1, 1], palette, (x) => (x === 0 ? 1 : 2));
    const mesh = meshChunk(stoneAndGlass, 0, 0, 0);
    // Stone keeps all six sides' worth (5 outside + the one against glass); glass is drawn on all six too.
    expect(mesh.opaque).not.toBeNull();
    expect(mesh.translucent).not.toBeNull();

    const glassPair = grid([2, 1, 1], palette, () => 2);
    expect(meshChunk(glassPair, 0, 0, 0).quads).toBe(6);
  });

  it("makes the hollow inside of a box visible only through the opening", () => {
    // A 3x3x3 shell of stone with the middle air: 6 outer + 6 inner faces.
    const g = grid([3, 3, 3], palette, (...at) => (at.every((v) => v === 1) ? 0 : 1));
    expect(meshChunk(g, 0, 0, 0).quads).toBe(12);
  });

  it("looks across chunk borders, so a seam is not drawn", () => {
    const g = grid([CHUNK * 2, 1, 1], palette, () => 1);
    const [nx] = chunkCounts(g);
    expect(nx).toBe(2);
    const left = meshChunk(g, 0, 0, 0);
    const right = meshChunk(g, 1, 0, 0);
    // Each half is a 16-long bar: 5 outward faces; the shared face is hidden on both sides.
    expect(left.quads).toBe(5);
    expect(right.quads).toBe(5);
  });

  it("puts vertex positions in world coordinates of the chunk, with matching colours", () => {
    const g = grid([CHUNK + 2, 1, 1], palette, () => 3);
    const mesh = meshChunk(g, 1, 0, 0);
    const xs = Array.from(mesh.opaque!.positions).filter((_, i) => i % 3 === 0);
    expect(Math.min(...xs)).toBe(CHUNK);
    expect(Math.max(...xs)).toBe(CHUNK + 2);
    expect(mesh.opaque!.colors.length).toBe(mesh.opaque!.positions.length);
    // The top face is the brightest: the gold block's own colour.
    const gold = blockLook("minecraft:gold_block").rgb;
    const colours = mesh.opaque!.colors;
    expect(Array.from(colours).some((_, i) => i % 3 === 0 && colours[i] === gold[0] && colours[i + 1] === gold[1])).toBe(true);
  });

  it("keeps every index inside the vertex array and every triangle facing outward", () => {
    const g = grid([4, 4, 4], palette, (x, y, z) => ((x + y + z) % 3 === 0 ? 0 : 1));
    const mesh = meshChunk(g, 0, 0, 0).opaque!;
    const vertexCount = mesh.positions.length / 3;
    for (const index of mesh.indices) {
      expect(index).toBeLessThan(vertexCount);
    }
    // For a single cube, every triangle's normal points away from the centre.
    const cube = meshChunk(grid([1, 1, 1], palette, () => 1), 0, 0, 0).opaque!;
    for (let t = 0; t < cube.indices.length; t += 3) {
      const p = [0, 1, 2].map((k) => Array.from(cube.positions.subarray(cube.indices[t + k] * 3, cube.indices[t + k] * 3 + 3)));
      const a = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
      const b = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
      const normal = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
      const toward = [p[0][0] - 0.5, p[0][1] - 0.5, p[0][2] - 0.5];
      expect(normal[0] * toward[0] + normal[1] * toward[1] + normal[2] * toward[2]).toBeGreaterThan(0);
    }
  });

  it("is cheap enough to do a chunk at a time: a worst-case checkerboard chunk stays bounded", () => {
    const g = grid([CHUNK, CHUNK, CHUNK], palette, (x, y, z) => ((x + y + z) % 2 === 0 ? 1 : 0));
    const started = performance.now();
    const mesh = meshChunk(g, 0, 0, 0);
    const elapsed = performance.now() - started;
    // 2048 cubes, nothing merges: 6 faces each.
    expect(mesh.quads).toBe(2048 * 6);
    // Generous: this is a correctness test, the real budget check is on the target machine.
    expect(elapsed).toBeLessThan(500);
  });
});

describe("pick", () => {
  const palette = [AIR, STONE, GOLD];

  it("finds the first block along a ray and says which face was hit", () => {
    const g = grid([4, 1, 1], palette, (x) => (x >= 2 ? 1 : 0));

    const hit = pick(g, [0.5, 0.5, 0.5], [1, 0, 0], 50);

    expect(hit).toMatchObject({ x: 2, y: 0, z: 0, palette: 1, normal: [-1, 0, 0] });
    expect(hit!.distance).toBeCloseTo(1.5, 5);
    expect(hit!.index).toBe(2);
  });

  it("hits the top of a block from above", () => {
    const g = grid([3, 3, 3], palette, (_x, y) => (y === 0 ? 1 : 0));
    const hit = pick(g, [1.5, 5, 1.5], [0, -1, 0], 50);
    expect(hit).toMatchObject({ x: 1, y: 0, z: 1, normal: [0, 1, 0] });
  });

  it("misses when nothing is in the way, outside the range or pointing away", () => {
    const g = grid([4, 1, 1], palette, (x) => (x === 3 ? 1 : 0));
    expect(pick(g, [0.5, 0.5, 0.5], [-1, 0, 0], 50)).toBeNull();
    expect(pick(g, [0.5, 0.5, 0.5], [1, 0, 0], 1)).toBeNull();
    expect(pick(g, [0.5, 5, 0.5], [1, 0, 0], 50)).toBeNull();
    expect(pick(g, [0, 0, 0], [0, 0, 0], 50)).toBeNull();
    expect(pick(g, [0, 0, 0], [Number.NaN, 0, 0], 50)).toBeNull();
  });

  it("starts from outside the box and from inside a block", () => {
    const g = grid([3, 3, 3], palette, () => 1);
    expect(pick(g, [-10, 1.5, 1.5], [1, 0, 0], 50)).toMatchObject({ x: 0, normal: [-1, 0, 0] });
    expect(pick(g, [1.5, 1.5, 1.5], [1, 0, 0], 50)).toMatchObject({ x: 1, y: 1, z: 1 });
  });

  it("skips air and blocks that cannot be pointed at", () => {
    const g = grid([3, 1, 1], [AIR, { name: "minecraft:air" }, GOLD], (x) => (x === 1 ? 1 : x === 2 ? 2 : 0));
    expect(pick(g, [0.5, 0.5, 0.5], [1, 0, 0], 50)).toMatchObject({ x: 2, palette: 2 });
  });

  it("walks diagonally through the grid without skipping a block", () => {
    const g = grid([8, 8, 8], palette, (x, y, z) => (x === 5 && y === 5 && z === 5 ? 2 : 0));
    const hit = pick(g, [0.1, 0.1, 0.1], [1, 1, 1], 50);
    expect(hit).toMatchObject({ x: 5, y: 5, z: 5, palette: 2 });
  });

  it("agrees with a brute-force march on random rays", () => {
    const g = grid([6, 6, 6], palette, (x, y, z) => ((x * 7 + y * 13 + z * 29) % 5 === 0 ? 1 : 0));
    let seed = 12345;
    const random = () => {
      seed = (Math.imul(seed, 1103515245) + 12345) >>> 0;
      return seed / 2 ** 32;
    };
    for (let i = 0; i < 300; i += 1) {
      const origin: [number, number, number] = [random() * 8 - 1, random() * 8 - 1, random() * 8 - 1];
      const dir: [number, number, number] = [random() - 0.5, random() - 0.5, random() - 0.5];
      const length = Math.hypot(...dir);
      let expected: number | null = null;
      for (let t = 0; t < 40 && expected === null; t += 0.002) {
        const x = Math.floor(origin[0] + (dir[0] / length) * t);
        const y = Math.floor(origin[1] + (dir[1] / length) * t);
        const z = Math.floor(origin[2] + (dir[2] / length) * t);
        const index = g.index(x, y, z);
        if (index >= 0 && g.blocks[index] === 1) {
          expected = index;
        }
      }
      const hit = pick(g, origin, dir, 40);
      expect(hit?.index ?? null).toBe(expected);
    }
  });
});

describe("camera", () => {
  const origin = { x: 0, y: 0, z: 0, yaw: 0, pitch: 0 };

  it("looks north at yaw 0 and turns right as the yaw grows", () => {
    expect(lookDirection(0, 0)).toEqual([0, 0, -1]);
    const east = lookDirection(Math.PI / 2, 0);
    expect(east[0]).toBeCloseTo(1, 5);
    expect(east[2]).toBeCloseTo(0, 5);
    expect(lookDirection(0, Math.PI / 4)[1]).toBeGreaterThan(0);
  });

  it("moves forward, sideways, up and down with WASD, Space and Shift", () => {
    const moved = (keys: Partial<typeof noKeys>) => stepFly(origin, { ...noKeys, ...keys }, 1, 10);
    expect(moved({ forward: true }).z).toBeCloseTo(-10, 5);
    expect(moved({ back: true }).z).toBeCloseTo(10, 5);
    expect(moved({ right: true }).x).toBeCloseTo(10, 5);
    expect(moved({ left: true }).x).toBeCloseTo(-10, 5);
    expect(moved({ up: true }).y).toBeCloseTo(10, 5);
    expect(moved({ down: true }).y).toBeCloseTo(-10, 5);
    expect(stepFly(origin, noKeys, 1, 10)).toBe(origin);
  });

  it("is no faster on a diagonal, and sprint is faster", () => {
    const diagonal = stepFly(origin, { ...noKeys, forward: true, right: true }, 1, 10);
    expect(Math.hypot(diagonal.x, diagonal.z)).toBeCloseTo(10, 5);
    const sprint = stepFly(origin, { ...noKeys, forward: true, sprint: true }, 1, 10);
    expect(sprint.z).toBeCloseTo(-30, 5);
  });

  it("moves along the facing direction on the ground plane and ignores the pitch", () => {
    const facingEast = { ...origin, yaw: Math.PI / 2, pitch: 1 };
    const moved = stepFly(facingEast, { ...noKeys, forward: true }, 1, 10);
    expect(moved.x).toBeCloseTo(10, 5);
    expect(moved.y).toBe(0);
  });

  it("scales with time and never moves backwards in time", () => {
    expect(stepFly(origin, { ...noKeys, forward: true }, 0.5, 10).z).toBeCloseTo(-5, 5);
    expect(stepFly(origin, { ...noKeys, forward: true }, -1, 10).z).toBeCloseTo(0, 5);
  });

  it("clamps the pitch and keeps the yaw in range", () => {
    const up = applyLook(origin, 0, -100000);
    expect(up.pitch).toBeCloseTo((89 * Math.PI) / 180, 5);
    const down = applyLook(origin, 0, 100000);
    expect(down.pitch).toBeCloseTo((-89 * Math.PI) / 180, 5);
    const spun = applyLook(origin, 100000, 0);
    expect(Math.abs(spun.yaw)).toBeLessThanOrEqual(Math.PI + 1e-9);
  });

  it("starts outside the build, looking at its centre", () => {
    const size: [number, number, number] = [20, 10, 30];
    const pose = startPose(size);
    const dir = lookDirection(pose.yaw, pose.pitch);
    const toCentre = [10 - pose.x, 5 - pose.y, 15 - pose.z];
    const length = Math.hypot(...toCentre);
    for (let axis = 0; axis < 3; axis += 1) {
      expect(dir[axis]).toBeCloseTo(toCentre[axis] / length, 4);
    }
    expect(pose.x > 20 || pose.z > 30).toBe(true);
  });

  it("flies faster over bigger builds", () => {
    expect(defaultSpeed([4, 4, 4])).toBe(6);
    expect(defaultSpeed([400, 10, 10])).toBe(40);
  });
});
