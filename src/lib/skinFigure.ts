/**
 * 把皮肤画成正对着看的整个人：头、身体、两只手臂、两条腿，每个部位先画里层再画外层
 * （帽子、外套、袖子、裤腿）。
 *
 * 皮肤是 64×64 的图；旧的 64×32 的图只有右手右腿，左边的是右边的镜像，没有外层（除了帽子）。
 * 经典手臂 4 像素宽，纤细手臂 3 像素宽。人是 16×32 个像素点，每个点放大到整数倍，最近邻取样。
 */

export type FigurePart = {
  /** 在皮肤图里的来源方块 */
  sx: number;
  sy: number;
  sw: number;
  sh: number;
  /** 在 16×32 的人里的位置 */
  dx: number;
  dy: number;
  /** 左右翻转（旧皮肤的左臂左腿是右边的镜像） */
  flip: boolean;
  overlay: boolean;
  /** 头上的帽子层（旧式皮肤要先看这块是不是实心，见 `legacyHatIsSolid`） */
  hat: boolean;
};

/** 人的大小，以皮肤的像素点计。 */
export const figureSize = { width: 16, height: 32 } as const;

/**
 * 要画的部位，按先后顺序。`modern` 是 64×64（有独立的左手左腿和外层），`slim` 是纤细手臂。
 */
export function figureParts(modern: boolean, slim: boolean): FigurePart[] {
  const arm = slim ? 3 : 4;
  const part = (
    sx: number,
    sy: number,
    sw: number,
    sh: number,
    dx: number,
    dy: number,
    overlay: boolean,
    flip = false,
  ): FigurePart => ({ sx, sy, sw, sh, dx, dy, flip, overlay, hat: false });

  const parts: FigurePart[] = [
    // 里层：右腿、左腿、身体、右臂、左臂、头
    part(4, 20, 4, 12, 4, 20, false),
    modern ? part(20, 52, 4, 12, 8, 20, false) : part(4, 20, 4, 12, 8, 20, false, true),
    part(20, 20, 8, 12, 4, 8, false),
    part(44, 20, arm, 12, 4 - arm, 8, false),
    modern ? part(36, 52, arm, 12, 12, 8, false) : part(44, 20, arm, 12, 12, 8, false, true),
    part(8, 8, 8, 8, 4, 0, false),
  ];
  if (modern) {
    // 外层：裤腿、外套、袖子
    parts.push(
      part(4, 36, 4, 12, 4, 20, true),
      part(4, 52, 4, 12, 8, 20, true),
      part(20, 36, 8, 12, 4, 8, true),
      part(44, 36, arm, 12, 4 - arm, 8, true),
      part(52, 52, arm, 12, 12, 8, true),
    );
  }
  // 帽子层两种皮肤都有
  parts.push({ ...part(40, 8, 8, 8, 4, 0, true), hat: true });
  return parts;
}

/**
 * 旧式（64×32）皮肤的帽子区域是不是整块实心。游戏对这种皮肤有一条老规矩（代码里叫
 * Notch 透明度 hack）：(32,0)–(64,32) 这一块里要是**没有一个**透明像素（alpha < 128），
 * 就当作没有帽子层，整块清成透明。Notch 的皮肤正是这样：这一块是不透明的黑色，不照这条
 * 规矩画，脸会被黑色盖住。`pixels` 是整张皮肤的 RGBA。
 */
export function legacyHatIsSolid(pixels: Uint8ClampedArray, width: number): boolean {
  for (let y = 0; y < 32; y += 1) {
    for (let x = 32; x < 64 && x < width; x += 1) {
      if (pixels[(y * width + x) * 4 + 3] < 128) {
        return false;
      }
    }
  }
  return true;
}

/** 整张皮肤的像素；读不出来（没有画布、被污染）返回 null。 */
function readPixels(skin: CanvasImageSource & { width: number; height: number }): Uint8ClampedArray | null {
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

/** `width`×`height` 的皮肤图是不是 64×64 的新式皮肤（旧式是 64×32）。 */
export function isModernSkin(width: number, height: number): boolean {
  return width === 64 && height === 64;
}

/** 把 `skin` 画成人，铺在 `target` 上（先清空，画布宽高要是 16×32 的整数倍）。画不了返回 false。 */
export function drawFigure(
  target: HTMLCanvasElement,
  skin: CanvasImageSource & { width: number; height: number },
  slim: boolean,
): boolean {
  const context = target.getContext("2d");
  if (context === null) {
    return false;
  }
  const scale = Math.max(1, Math.floor(target.width / figureSize.width));
  context.clearRect(0, 0, target.width, target.height);
  context.imageSmoothingEnabled = false;
  const modern = isModernSkin(skin.width, skin.height);
  // 旧式皮肤的帽子区域整块实心就不画帽子；读不出像素时照画（宁可多画一层也不画错别的）
  const pixels = modern ? null : readPixels(skin);
  const skipHat = pixels !== null && legacyHatIsSolid(pixels, skin.width);
  for (const part of figureParts(modern, slim)) {
    if (part.hat && skipHat) {
      continue;
    }
    const x = part.dx * scale;
    const y = part.dy * scale;
    const w = part.sw * scale;
    const h = part.sh * scale;
    if (part.flip) {
      context.save();
      context.translate(x + w, y);
      context.scale(-1, 1);
      context.drawImage(skin, part.sx, part.sy, part.sw, part.sh, 0, 0, w, h);
      context.restore();
    } else {
      context.drawImage(skin, part.sx, part.sy, part.sw, part.sh, x, y, w, h);
    }
  }
  return true;
}
