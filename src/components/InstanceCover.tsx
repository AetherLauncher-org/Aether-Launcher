import type { CSSProperties } from "react";
import { coverHue } from "../lib/instances";

type InstanceCoverProps = {
  version: string;
  /** 方块图案大小，默认铺满容器 */
  className?: string;
};

/**
 * 实例的封面。没有用户上传的图片，所以用版本号生成一个稳定的色相，
 * 叠上等距方块的轮廓——同一个版本永远是同一个颜色，一眼能分辨。
 */
export function InstanceCover({ version, className }: InstanceCoverProps) {
  const style = { "--cover-hue": coverHue(version) } as CSSProperties;
  return (
    <div className={`instance-cover${className ? ` ${className}` : ""}`} style={style} aria-hidden="true">
      <svg viewBox="0 0 120 120" className="instance-cover-art" focusable="false">
        <polygon points="60,18 98,40 60,62 22,40" className="cover-top" />
        <polygon points="22,40 60,62 60,104 22,82" className="cover-left" />
        <polygon points="98,40 98,82 60,104 60,62" className="cover-right" />
      </svg>
    </div>
  );
}
