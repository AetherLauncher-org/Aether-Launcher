import { useEffect, useRef, useState } from "react";
import { getSkinImage } from "../ipc";
import { drawFigure, figureSize } from "../lib/skinFigure";

type SkinFigureProps = {
  /** 皮肤库里的 id（图片内容的 SHA-256） */
  skinId: string;
  slim: boolean;
  /** 一个皮肤像素点放大几倍，默认 6（人是 96×192） */
  scale?: number;
};

/**
 * 皮肤库里一张皮肤，画成正对着看的整个人。图由后端按 id 给（已验过的 `data:` PNG），
 * 在 2D 画布上画；没有画布（测试环境）或图取不到时留一个空位，不报错。
 */
export function SkinFigure({ skinId, slim, scale = 6 }: SkinFigureProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [loaded, setLoaded] = useState<{ id: string; image: HTMLImageElement } | null>(null);

  useEffect(() => {
    let active = true;
    void getSkinImage(skinId)
      .then(
        (url) =>
          new Promise<HTMLImageElement | null>((resolve) => {
            const image = new Image();
            image.onload = () => resolve(image);
            image.onerror = () => resolve(null);
            image.src = url;
          }),
      )
      .then((image) => {
        if (active && image !== null) {
          setLoaded({ id: skinId, image });
        }
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [skinId]);

  const image = loaded !== null && loaded.id === skinId ? loaded.image : null;

  useEffect(() => {
    if (image !== null && canvas.current !== null) {
      drawFigure(canvas.current, image, slim);
    }
  }, [image, slim]);

  return (
    <canvas
      ref={canvas}
      className="skin-figure"
      width={figureSize.width * scale}
      height={figureSize.height * scale}
      style={{ width: figureSize.width * scale, height: figureSize.height * scale }}
      aria-hidden="true"
    />
  );
}
