import type { SchematicView } from "../../ipc/schematics";
import { decodeRuns } from "./decode";
import { VoxelGrid } from "./grid";
import { IsoJob, MAX_THUMBNAIL_VOLUME } from "./thumbnail";
import type { DrawContext } from "./thumbnail";

/** 卡片上显示的缩略图尺寸（CSS 像素）。 */
export const THUMBNAIL_WIDTH = 224;
export const THUMBNAIL_HEIGHT = 140;

/** 一张缩略图每一帧最多画这么多毫秒。 */
const SLICE_MS = 3;

export type PreviewSummary = {
  format: SchematicView["format"];
  size: [number, number, number];
  solidBlocks: number;
  dataVersion: number | null;
  blockEntityTotal: number;
  name: string;
  author: string;
};

export type Preview = {
  summary: PreviewSummary;
  /** `data:image/png;base64,…`；画不了（体积太大、没有画布）时为 null */
  image: string | null;
};

/** 画布和调度，测试里换成假的。 */
export type PreviewEnvironment = {
  createCanvas: (width: number, height: number) => { getContext: (kind: "2d") => DrawContext | null; toDataURL: () => string };
  /** 下一帧再继续。 */
  nextFrame: (callback: () => void) => void;
  now: () => number;
};

const browserEnvironment: PreviewEnvironment = {
  createCanvas: (width, height) => {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    return canvas as unknown as ReturnType<PreviewEnvironment["createCanvas"]>;
  },
  nextFrame: (callback) => {
    requestAnimationFrame(() => callback());
  },
  now: () => performance.now(),
};

export function summarise(view: SchematicView): PreviewSummary {
  return {
    format: view.format,
    size: view.size,
    solidBlocks: view.solidBlocks,
    dataVersion: view.dataVersion,
    blockEntityTotal: view.blockEntityTotal,
    name: view.name,
    author: view.author,
  };
}

/** 画出缩略图；画不了返回 null。分段执行，两段之间把时间还给页面。 */
export async function drawThumbnail(view: SchematicView, env: PreviewEnvironment = browserEnvironment): Promise<string | null> {
  const [sx, sy, sz] = view.size;
  if (sx * sy * sz > MAX_THUMBNAIL_VOLUME || sx * sy * sz === 0) {
    return null;
  }
  const canvas = env.createCanvas(THUMBNAIL_WIDTH * 2, THUMBNAIL_HEIGHT * 2);
  const context = canvas.getContext("2d");
  if (context === null) {
    return null;
  }
  const grid = new VoxelGrid(sx, sy, sz, decodeRuns(view.runs, view.runCount, sx * sy * sz), view.palette);
  const job = new IsoJob(grid, context, THUMBNAIL_WIDTH * 2, THUMBNAIL_HEIGHT * 2, 10);
  await new Promise<void>((resolve) => {
    const slice = () => {
      if (job.run(env.now() + SLICE_MS, env.now)) {
        resolve();
      } else {
        env.nextFrame(slice);
      }
    };
    slice();
  });
  return canvas.toDataURL();
}

/**
 * 缩略图一张一张地做：读文件、解码、画。同时只有一张在进行，页面上有再多卡片也不会
 * 同时占满内存和主线程。结果按 key 缓存，重复请求同一张直接返回同一个 Promise。
 */
export class PreviewQueue {
  private readonly cache = new Map<string, Promise<Preview>>();
  private tail: Promise<unknown> = Promise.resolve();

  constructor(private readonly env: PreviewEnvironment = browserEnvironment) {}

  request(key: string, load: () => Promise<SchematicView>): Promise<Preview> {
    const cached = this.cache.get(key);
    if (cached !== undefined) {
      return cached;
    }
    const job = this.tail.then(async () => {
      const view = await load();
      let image: string | null = null;
      try {
        image = await drawThumbnail(view, this.env);
      } catch {
        // 缩略图画不出来不影响信息显示
      }
      return { summary: summarise(view), image };
    });
    this.tail = job.catch(() => undefined);
    // 失败的不缓存，下次进来可以重试。
    const guarded = job.catch((error: unknown) => {
      this.cache.delete(key);
      throw error;
    });
    this.cache.set(key, guarded);
    return guarded;
  }

  forget(prefix: string): void {
    for (const key of [...this.cache.keys()]) {
      if (key.startsWith(prefix)) {
        this.cache.delete(key);
      }
    }
  }
}

/** 整个应用共用一个队列。 */
export const previews = new PreviewQueue();
