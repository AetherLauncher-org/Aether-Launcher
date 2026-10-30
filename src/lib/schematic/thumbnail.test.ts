import { describe, expect, it } from "vitest";
import { VoxelGrid } from "./grid";
import { IsoJob } from "./thumbnail";
import type { DrawContext } from "./thumbnail";

const palette = [{ name: "minecraft:air" }, { name: "minecraft:stone" }, { name: "minecraft:glass" }];

function grid(size: [number, number, number], at: (x: number, y: number, z: number) => number) {
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

function recorder() {
  const calls = { fills: 0, alphas: [] as number[], points: [] as [number, number][] };
  const ctx: DrawContext = {
    fillStyle: "",
    get globalAlpha() {
      return 1;
    },
    set globalAlpha(value: number) {
      calls.alphas.push(value);
    },
    beginPath() {},
    moveTo(x, y) {
      calls.points.push([x, y]);
    },
    lineTo(x, y) {
      calls.points.push([x, y]);
    },
    closePath() {},
    fill() {
      calls.fills += 1;
    },
  };
  return { ctx, calls };
}

describe("IsoJob", () => {
  it("draws the three visible faces of a lone block", () => {
    const { ctx, calls } = recorder();
    const job = new IsoJob(grid([1, 1, 1], () => 1), ctx, 100, 100);

    expect(job.run(Infinity)).toBe(true);

    expect(job.done).toBe(true);
    expect(calls.fills).toBe(3);
    expect(job.quads).toBe(3);
  });

  it("skips faces that a neighbour hides", () => {
    const { ctx } = recorder();
    // Two blocks side by side along x: the +x face of the first and nothing else is hidden.
    const job = new IsoJob(grid([2, 1, 1], () => 1), ctx, 100, 100);
    job.run(Infinity);
    // Block 0: top + south (its +x face is hidden). Block 1: top + east + south.
    expect(job.quads).toBe(5);
  });

  it("draws nothing for air, and glass at half opacity", () => {
    const air = recorder();
    new IsoJob(grid([3, 3, 3], () => 0), air.ctx, 100, 100).run(Infinity);
    expect(air.calls.fills).toBe(0);

    const glass = recorder();
    new IsoJob(grid([1, 1, 1], () => 2), glass.ctx, 100, 100).run(Infinity);
    expect(glass.calls.alphas).toContain(0.55);
  });

  it("keeps every point inside the picture", () => {
    const { ctx, calls } = recorder();
    new IsoJob(grid([5, 4, 7], () => 1), ctx, 160, 120).run(Infinity);

    for (const [x, y] of calls.points) {
      expect(x).toBeGreaterThanOrEqual(-0.5);
      expect(x).toBeLessThanOrEqual(160.5);
      expect(y).toBeGreaterThanOrEqual(-0.5);
      expect(y).toBeLessThanOrEqual(120.5);
    }
  });

  it("draws back to front: lower layers first, and nearer blocks after farther ones", () => {
    const order: string[] = [];
    const { ctx } = recorder();
    const job = new IsoJob(grid([2, 2, 2], () => 1), ctx, 100, 100);
    const original = (job as unknown as { block: (x: number, y: number, z: number) => void }).block.bind(job);
    (job as unknown as { block: (x: number, y: number, z: number) => void }).block = (x, y, z) => {
      order.push(`${y}:${x + z}`);
      original(x, y, z);
    };

    job.run(Infinity);

    const keys = order.map((entry) => entry.split(":").map(Number));
    for (let i = 1; i < keys.length; i += 1) {
      expect(keys[i][0] > keys[i - 1][0] || (keys[i][0] === keys[i - 1][0] && keys[i][1] >= keys[i - 1][1])).toBe(true);
    }
    expect(order).toHaveLength(8);
  });

  it("can be paused and resumed and ends up with the same drawing", () => {
    const shape = (x: number, y: number, z: number) => ((x + y * 2 + z) % 3 === 0 ? 1 : 0);
    const whole = recorder();
    new IsoJob(grid([9, 5, 9], shape), whole.ctx, 200, 200).run(Infinity);

    const sliced = recorder();
    const job = new IsoJob(grid([9, 5, 9], shape), sliced.ctx, 200, 200);
    let slices = 0;
    // A clock that says "time is up" on every check, so each call does 64 blocks at most.
    while (!job.run(0, () => 1)) {
      slices += 1;
      expect(slices).toBeLessThan(10_000);
    }

    expect(slices).toBeGreaterThan(1);
    expect(sliced.calls.fills).toBe(whole.calls.fills);
    expect(sliced.calls.points).toEqual(whole.calls.points);
  });

  it("an already finished job does nothing more", () => {
    const { ctx, calls } = recorder();
    const job = new IsoJob(grid([1, 1, 1], () => 1), ctx, 50, 50);
    job.run(Infinity);
    const fills = calls.fills;

    expect(job.run(Infinity)).toBe(true);
    expect(calls.fills).toBe(fills);
  });
});
