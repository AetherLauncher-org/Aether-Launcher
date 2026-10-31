/**
 * 把皮肤画成一个斜 45° 的 3D 头：里层的头加外层（帽子层）。
 *
 * 皮肤是 64×64（旧的是 64×32）的图。头的六个面各占 8×8 像素：顶面 (8,0)，正面 (8,8)，
 * 角色左侧 (16,8)；外层在同样的位置加 32 个像素（顶面 (40,0)，正面 (40,8)，左侧 (48,8)），
 * 比里层大 1/8（`overlayScale`），所以会罩在里层外面。画的时候只画看得见的三个面：
 * 正面在左，侧面在右，顶面在上，三个面各自用一个仿射变换把 8×8 的方块贴到菱形或平行四边形上。
 */

const COS30 = Math.sqrt(3) / 2;

/** 外层比里层大 1/8。 */
export const overlayScale = 9 / 8;

export type Matrix = [number, number, number, number, number, number];

export type HeadFace = {
  /** 在皮肤图里的 8×8 方块的左上角 */
  sx: number;
  sy: number;
  /** `ctx.setTransform` 的参数：把方块的像素坐标贴到画布上 */
  matrix: Matrix;
  /** 加多少黑色（0–1），侧面比顶面暗一点，立体感就出来了 */
  shade: number;
  /** 是不是外层 */
  overlay: boolean;
};

/**
 * 一个 `size`×`size` 的画布上要画的面，按先后顺序：里层的顶、正、侧，再是外层的顶、正、侧。
 * 头（连外层）刚好顶到画布的上下边，水平居中。
 */
export function headFaces(size: number): HeadFace[] {
  const centre = size / 2;
  const edge = size / (2 * overlayScale);
  const faces: HeadFace[] = [];
  for (const overlay of [false, true]) {
    const length = overlay ? edge * overlayScale : edge;
    const across = (COS30 * length) / 8;
    const rise = (0.5 * length) / 8;
    const down = length / 8;
    const x = overlay ? 32 : 0;
    faces.push(
      { sx: 8 + x, sy: 0, matrix: [across, rise, -across, rise, centre, centre - length], shade: 0, overlay },
      {
        sx: 8 + x,
        sy: 8,
        matrix: [across, rise, 0, down, centre - COS30 * length, centre - 0.5 * length],
        shade: 0.15,
        overlay,
      },
      { sx: 16 + x, sy: 8, matrix: [across, -rise, 0, down, centre, centre], shade: 0.35, overlay },
    );
  }
  return faces;
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
  const scratch = document.createElement("canvas");
  scratch.width = target.width;
  scratch.height = target.height;
  const layer = scratch.getContext("2d");
  if (context === null || layer === null) {
    return false;
  }
  context.clearRect(0, 0, target.width, target.height);
  for (const face of headFaces(target.width)) {
    layer.setTransform(1, 0, 0, 1, 0, 0);
    layer.globalCompositeOperation = "source-over";
    layer.clearRect(0, 0, scratch.width, scratch.height);
    layer.imageSmoothingEnabled = false;
    layer.setTransform(...face.matrix);
    layer.drawImage(skin, face.sx, face.sy, 8, 8, 0, 0, 8, 8);
    layer.setTransform(1, 0, 0, 1, 0, 0);
    if (face.shade > 0) {
      // 只给画了像素的地方加暗，外层透明的地方不能被染黑
      layer.globalCompositeOperation = "source-atop";
      layer.fillStyle = `rgba(0, 0, 0, ${face.shade})`;
      layer.fillRect(0, 0, scratch.width, scratch.height);
    }
    context.drawImage(scratch, 0, 0);
  }
  return true;
}
