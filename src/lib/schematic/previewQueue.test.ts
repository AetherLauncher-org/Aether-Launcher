import { describe, expect, it, vi } from "vitest";
import type { SchematicView } from "../../ipc/schematics";
import { PreviewQueue, drawThumbnail, summarise } from "./previewQueue";
import type { PreviewEnvironment } from "./previewQueue";

function toBase64(runs: [number, number][]): string {
  const bytes = runs.flatMap(([value, count]) => [value & 255, value >> 8, 0, 0, count & 255, (count >> 8) & 255, 0, 0]);
  return btoa(String.fromCharCode(...bytes));
}

function view(overrides: Partial<SchematicView> = {}): SchematicView {
  return {
    fileName: "House.litematic",
    format: "litematic",
    name: "House",
    author: "Aether",
    description: "",
    dataVersion: 3955,
    size: [2, 2, 2],
    solidBlocks: 8,
    palette: [
      { name: "minecraft:air", properties: [], air: true, count: 0 },
      { name: "minecraft:stone", properties: [], air: false, count: 8 },
    ],
    runs: toBase64([[1, 8]]),
    runCount: 1,
    blockEntities: [],
    blockEntityTotal: 0,
    ...overrides,
  };
}

function environment(options: { context?: boolean; step?: number } = {}) {
  const frames: (() => void)[] = [];
  const fills = { count: 0 };
  const context = {
    fillStyle: "",
    globalAlpha: 1,
    beginPath() {},
    moveTo() {},
    lineTo() {},
    closePath() {},
    fill() {
      fills.count += 1;
    },
  };
  let clock = 0;
  const env: PreviewEnvironment = {
    createCanvas: () => ({
      getContext: () => (options.context === false ? null : context),
      toDataURL: () => "data:image/png;base64,AAAA",
    }),
    nextFrame: (callback) => frames.push(callback),
    // Every look at the clock moves it on, so a slice ends after a few blocks.
    now: () => {
      clock += options.step ?? 0;
      return clock;
    },
  };
  return { env, frames, fills };
}

describe("drawThumbnail", () => {
  it("draws the visible faces and returns the picture", async () => {
    const { env, fills } = environment();

    const image = await drawThumbnail(view(), env);

    expect(image).toBe("data:image/png;base64,AAAA");
    // A 2x2x2 cube shows 4 top, 4 east and 4 south faces.
    expect(fills.count).toBe(12);
  });

  it("spreads a big drawing over several frames instead of one long one", async () => {
    const side = 12;
    const big = view({ size: [side, side, side], runs: toBase64([[1, side ** 3]]), solidBlocks: side ** 3 });
    const { env, frames } = environment({ step: 10 });

    const done = drawThumbnail(big, env);
    // Keep handing the page back until the drawing says it is finished.
    for (let guard = 0; guard < 1000; guard += 1) {
      const next = frames.shift();
      if (next === undefined) {
        break;
      }
      next();
    }
    await done;

    expect(frames).toHaveLength(0);
  });

  it("gives up quietly on a schematic that is too big, or has no canvas", async () => {
    const big = view({ size: [400, 400, 400] });
    expect(await drawThumbnail(big, environment().env)).toBeNull();
    expect(await drawThumbnail(view(), environment({ context: false }).env)).toBeNull();
    expect(await drawThumbnail(view({ size: [0, 1, 1] }), environment().env)).toBeNull();
  });

  it("refuses data that does not match its size", async () => {
    await expect(drawThumbnail(view({ runs: toBase64([[1, 3]]) }), environment().env)).rejects.toThrow();
  });
});

describe("PreviewQueue", () => {
  it("returns a summary and the picture, and loads each file once", async () => {
    const queue = new PreviewQueue(environment().env);
    const load = vi.fn(async () => view());

    const first = await queue.request("a/House.litematic", load);
    const again = await queue.request("a/House.litematic", load);

    expect(first.summary).toEqual(summarise(view()));
    expect(first.image).toBe("data:image/png;base64,AAAA");
    expect(again).toBe(first);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("works on one file at a time, in the order asked", async () => {
    const queue = new PreviewQueue(environment().env);
    const order: string[] = [];
    let running = 0;
    let most = 0;
    const loader = (name: string) => async () => {
      running += 1;
      most = Math.max(most, running);
      order.push(name);
      await new Promise((resolve) => setTimeout(resolve, 5));
      running -= 1;
      return view({ name });
    };

    await Promise.all([
      queue.request("1", loader("one")),
      queue.request("2", loader("two")),
      queue.request("3", loader("three")),
    ]);

    expect(order).toEqual(["one", "two", "three"]);
    expect(most).toBe(1);
  });

  it("still describes the file when the picture cannot be drawn", async () => {
    const queue = new PreviewQueue(environment({ context: false }).env);

    const preview = await queue.request("k", async () => view());

    expect(preview.image).toBeNull();
    expect(preview.summary.size).toEqual([2, 2, 2]);
  });

  it("does not cache a failure, and one failure does not stop the others", async () => {
    const queue = new PreviewQueue(environment().env);

    await expect(queue.request("bad", async () => Promise.reject(new Error("unreadable")))).rejects.toThrow("unreadable");
    const retry = await queue.request("bad", async () => view());
    const other = await queue.request("good", async () => view());

    expect(retry.summary.name).toBe("House");
    expect(other.image).not.toBeNull();
  });

  it("forgets what starts with a prefix, so a deleted or converted file is read again", async () => {
    const queue = new PreviewQueue(environment().env);
    const load = vi.fn(async () => view());
    await queue.request("i1/House.litematic", load);
    await queue.request("i2/House.litematic", load);

    queue.forget("i1/");
    await queue.request("i1/House.litematic", load);
    await queue.request("i2/House.litematic", load);

    expect(load).toHaveBeenCalledTimes(3);
  });
});
