/**
 * 把皮肤画成正对着看的头：里层的脸加外层（帽子层），不画顶面和侧面。
 *
 * 皮肤是 64×64（旧的是 64×32）的图。头的正面是 (8,8) 起的 8×8 像素，外层的正面在同样的
 * 位置加 32 个像素，即 (40,8)。外层比里层大 1/8（`overlayScale`），所以罩在里层外面一圈，
 * 刚好是游戏里帽子层的样子。两层都按整数倍之外的比例放大，用最近邻取样，保持像素的方块感。
 */

/** 外层比里层大 1/8。 */
export const overlayScale = 9 / 8;

export type Matrix = [number, number, number, number, number, number];

export type HeadFace = {
  /** 在皮肤图里的 8×8 方块的左上角 */
  sx: number;
  sy: number;
  /** `ctx.setTransform` 的参数：把方块的像素坐标贴到画布上 */
  matrix: Matrix;
  /** 加多少黑色（0–1）；正面不加暗，保留皮肤原本的颜色 */
  shade: number;
  /** 是不是外层 */
  overlay: boolean;
};

/**
 * 一个 `size`×`size` 的画布上要画的面，按先后顺序：里层的脸，再是外层的脸。外层铺满画布，
 * 里层在正中，小 1/8。
 */
export function headFaces(size: number): HeadFace[] {
  return [false, true].map((overlay) => {
    const length = overlay ? size : size / overlayScale;
    const offset = (size - length) / 2;
    const step = length / 8;
    return {
      sx: overlay ? 40 : 8,
      sy: 8,
      matrix: [step, 0, 0, step, offset, offset] as Matrix,
      shade: 0,
      overlay,
    };
  });
}

/** 这台机器画得了吗（没有 2D 画布的环境，比如测试，退回字母头像）。 */
export function canDrawHead(): boolean {
  try {
    return document.createElement("canvas").getContext("2d") !== null;
  } catch {
    return false;
  }
}

/** 把 `skin` 的头画到 `target` 上（先清空）。画不了时返回 false。 */
export function drawHead(target: HTMLCanvasElement, skin: CanvasImageSource): boolean {
  const context = target.getContext("2d");
  if (context === null) {
    return false;
  }
  context.clearRect(0, 0, target.width, target.height);
  context.imageSmoothingEnabled = false;
  for (const face of headFaces(target.width)) {
    context.setTransform(...face.matrix);
    context.drawImage(skin, face.sx, face.sy, 8, 8, 0, 0, 8, 8);
  }
  context.setTransform(1, 0, 0, 1, 0, 0);
  return true;
}
