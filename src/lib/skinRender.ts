/**
 * 把皮肤画成一个有体积的小人（几何见 `skinModel.ts`）。自己做一个很小的软件渲染器：
 * 正交投影，每个方块逐像素比深度（z-buffer），不用 WebGL，也不加任何三维库，
 * 低配的机器上也画得动。逐像素比深度所以从任何角度看遮挡都对（不会出现里层透出来），
 * 也不需要排序。画出来的是纯色不透明的像素，和游戏里「镂空」的画法一致。
 */

import {
  boxQuads,
  isModernSkin,
  legacyHatIsSolid,
  outerVoxels,
  skinGroups,
  transform,
  viewFromAngles,
  voxelQuads,
  withOpaqueBase,
} from "./skinModel";
import type { Mat3, Quad, Vec3 } from "./skinModel";

export type SkinImage = CanvasImageSource & { width: number; height: number };

/** 人在画布里占的宽和高，以皮肤的像素计（含外层凸出的部分和一点边距）。 */
export const viewSize = { width: 22, height: 36 } as const;

/** 平时的角度：稍微侧一点、稍微俯一点，这样看得出外层的厚度。 */
export const defaultView: Mat3 = viewFromAngles(-0.5, 0.2);

/** 光从左上前方来（观察者的坐标：x 向右，y 向上，z 朝向观察者）。 */
const light: Vec3 = (() => {
  const v: Vec3 = [-0.35, 0.8, 0.5];
  const length = Math.hypot(...v);
  return [v[0] / length, v[1] / length, v[2] / length];
})();

/** 朝向 `normal`（观察者坐标）的面有多亮，0.58–1。 */
function shade(normal: Vec3): number {
  const dot = normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2];
  return 0.58 + 0.42 * Math.max(0, dot);
}

/**
 * 读好像素、拆成四边形的皮肤。四边形只算一次，转动时只重新投影。`bounds` 是整个人（连外层凸
 * 出的部分）的一圈角点，用来在转到比画布还大的姿势时自动缩小，不被切掉。
 */
export type PreparedSkin = { quads: readonly Quad[]; bounds?: readonly Vec3[] };

/** 整张皮肤的像素；读不出来（没有画布、被污染）返回 null。 */
function readPixels(skin: SkinImage): Uint8ClampedArray | null {
  try {
    const canvas = document.createElement("canvas");
    canvas.width = skin.width;
    canvas.height = skin.height;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (context === null) {
      return null;
    }
    context.drawImage(skin, 0, 0);
    return context.getImageData(0, 0, skin.width, skin.height).data;
  } catch {
    return null;
  }
}

/** 从皮肤的像素建模型。 */
export function prepareFromPixels(pixels: Uint8ClampedArray, width: number, height: number, slim: boolean): PreparedSkin {
  const modern = isModernSkin(width, height);
  // 里层在游戏里是强制不透明的；外层保持原来的 alpha，实心与否按「镂空」的规矩判断
  const base = withOpaqueBase(pixels, width, height);
  // 旧式皮肤的帽子区域整块实心：游戏当作没有帽子（Notch 的皮肤），见 `legacyHatIsSolid`
  const hatSolid = !modern && legacyHatIsSolid(pixels, width);
  const quads: Quad[] = [];
  const bounds: Vec3[] = [];
  for (const group of skinGroups(modern, slim)) {
    quads.push(...boxQuads(group.base, base, width));
    // 外层最多凸出 `thickness`，边缘的体素还向边外伸 `thickness`：都算进范围
    const grow = group.outer === null ? 0 : group.thickness;
    const { x0, x1, y0, y1, z0, z1 } = group.base;
    for (const x of [x0 - grow, x1 + grow]) {
      for (const y of [y0 - grow, y1 + grow]) {
        for (const z of [z0 - grow, z1 + grow]) {
          bounds.push([x, y, z]);
        }
      }
    }
    if (group.outer !== null && !(group.outer.name === "hat" && hatSolid)) {
      for (const voxel of outerVoxels(group.outer, group.thickness, pixels, width)) {
        quads.push(...voxelQuads(voxel));
      }
    }
  }
  return { quads, bounds };
}

/** 读皮肤的像素并建好模型；读不出来返回 null（画不了）。 */
export function prepareSkin(image: SkinImage, slim: boolean): PreparedSkin | null {
  const pixels = readPixels(image);
  return pixels === null ? null : prepareFromPixels(pixels, image.width, image.height, slim);
}

/** 画好的一帧：RGBA 像素，和每个像素离观察者的距离（越大越近，没画到的是 -Infinity）。 */
export type Frame = { pixels: Uint8ClampedArray<ArrayBuffer>; depth: Float32Array };

/**
 * 把 `skin` 用 `view` 这个旋转画到 `width`×`height` 的像素上。这个函数不碰画布，只算像素，
 * 所以可以直接测试。每个四边形先剔除背面，再逐像素比深度；同一个像素只留最近的。
 */
export function rasterize(skin: PreparedSkin, view: Mat3, width: number, height: number): Frame {
  const pixels = new Uint8ClampedArray(new ArrayBuffer(width * height * 4));
  const depth = new Float32Array(width * height).fill(Number.NEGATIVE_INFINITY);
  const centreX = width / 2;
  const centreY = height / 2;
  // 以人的中心（y = 16）为转动的中心
  const centred = (p: Vec3): Vec3 => transform(view, [p[0], p[1] - 16, p[2]]);
  // 平时按 `viewSize` 的大小画；转到比画布还大的姿势（比如翻滚 90°，人比画布宽）就缩小到刚好放下
  let scale = Math.min(width / viewSize.width, height / viewSize.height);
  if (skin.bounds !== undefined && skin.bounds.length > 0) {
    let reachX = 0;
    let reachY = 0;
    for (const corner of skin.bounds) {
      const r = centred(corner);
      reachX = Math.max(reachX, Math.abs(r[0]));
      reachY = Math.max(reachY, Math.abs(r[1]));
    }
    const margin = 0.96;
    scale = Math.min(scale, (centreX * margin) / Math.max(reachX, 1e-6), (centreY * margin) / Math.max(reachY, 1e-6));
  }
  const project = (p: Vec3): Vec3 => {
    const r = centred(p);
    return [centreX + r[0] * scale, centreY - r[1] * scale, r[2]];
  };
  /** 跨过边界的容差：相邻两个四边形共用一条边，边上的像素中心不能两边都不要 */
  const eps = 0.02;

  for (const quad of skin.quads) {
    const normal = transform(view, quad.normal);
    if (normal[2] <= 0.001) {
      continue; // 背着观察者
    }
    const p0 = project(quad.corners[0]);
    const p1 = project(quad.corners[1]);
    const p2 = project(quad.corners[2]);
    const p3 = project(quad.corners[3]);
    const e1x = p1[0] - p0[0];
    const e1y = p1[1] - p0[1];
    const e2x = p3[0] - p0[0];
    const e2y = p3[1] - p0[1];
    const det = e1x * e2y - e1y * e2x;
    if (Math.abs(det) < 1e-9) {
      continue; // 侧着看成了一条线
    }
    const dz1 = p1[2] - p0[2];
    const dz2 = p3[2] - p0[2];

    const minX = Math.max(0, Math.floor(Math.min(p0[0], p1[0], p2[0], p3[0]) - 0.5));
    const maxX = Math.min(width - 1, Math.ceil(Math.max(p0[0], p1[0], p2[0], p3[0]) - 0.5));
    const minY = Math.max(0, Math.floor(Math.min(p0[1], p1[1], p2[1], p3[1]) - 0.5));
    const maxY = Math.min(height - 1, Math.ceil(Math.max(p0[1], p1[1], p2[1], p3[1]) - 0.5));

    const level = shade(normal);
    const red = quad.color[0] * level;
    const green = quad.color[1] * level;
    const blue = quad.color[2] * level;

    for (let y = minY; y <= maxY; y += 1) {
      const dy = y + 0.5 - p0[1];
      for (let x = minX; x <= maxX; x += 1) {
        const dx = x + 0.5 - p0[0];
        // 像素中心在平行四边形里的位置：p0 + s·e1 + t·e2
        const s = (dx * e2y - dy * e2x) / det;
        const t = (e1x * dy - e1y * dx) / det;
        if (s < -eps || s > 1 + eps || t < -eps || t > 1 + eps) {
          continue;
        }
        const z = p0[2] + s * dz1 + t * dz2;
        const index = y * width + x;
        if (z > depth[index]) {
          depth[index] = z;
          pixels[index * 4] = red;
          pixels[index * 4 + 1] = green;
          pixels[index * 4 + 2] = blue;
          pixels[index * 4 + 3] = 255;
        }
      }
    }
  }
  return { pixels, depth };
}

/** 画一帧到画布上（画布的宽高要和 `viewSize` 同比例）。画不了（没有 2D 画布）返回 false。 */
export function renderSkin(target: HTMLCanvasElement, skin: PreparedSkin, view: Mat3): boolean {
  const context = target.getContext("2d");
  if (context === null) {
    return false;
  }
  const frame = rasterize(skin, view, target.width, target.height);
  context.setTransform(1, 0, 0, 1, 0, 0);
  context.putImageData(new ImageData(frame.pixels, target.width, target.height), 0, 0);
  return true;
}
