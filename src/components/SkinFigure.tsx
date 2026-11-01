import { useEffect, useState } from "react";
import { getSkinImage } from "../ipc";
import { SkinView } from "./SkinView";

type SkinFigureProps = {
  /** 皮肤库里的 id（图片内容的 SHA-256） */
  skinId: string;
  slim: boolean;
  /** 一个皮肤像素点放大几倍，默认 6 */
  scale?: number;
};

/**
 * 皮肤库里一张皮肤，画成有体积的小人（见 `SkinView`）。图由后端按 id 给（已验过的 `data:`
 * PNG）；图取不到时留一个空位，不报错。
 */
export function SkinFigure({ skinId, slim, scale = 6 }: SkinFigureProps) {
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

  return <SkinView image={loaded !== null && loaded.id === skinId ? loaded.image : null} slim={slim} scale={scale} />;
}
