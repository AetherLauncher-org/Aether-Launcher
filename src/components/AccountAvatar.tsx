import { Person20Regular } from "@fluentui/react-icons";
import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { coverHue } from "../lib/instances";
import { loadSkin } from "../lib/skin";
import { drawHead } from "../lib/skinHead";

type AccountAvatarProps = {
  /** 账户的玩家名；没有账户时传 null，显示一个中性的人像 */
  name: string | null;
  /** 微软账户的玩家 UUID（32 位小写十六进制）：有的话画皮肤的 3D 头；离线账户不传 */
  uuid?: string | null;
  size?: number;
};

/**
 * 账户头像。微软账户画玩家皮肤的头（正对着看，里层的脸加外层的帽子层，见 `lib/skinHead.ts`）；
 * 离线账户没有皮肤可取，微软账户的皮肤取不到（没网、没有自定义皮肤）时，退回玩家名首字母
 * 加一个由名字决定的色相，同一个名字永远是同一种颜色，和实例封面用同一套规则。
 */
export function AccountAvatar({ name, uuid = null, size = 40 }: AccountAvatarProps) {
  // 按 UUID 记住取到的图：换了账户，旧账户的头不会短暂地出现在新账户上
  const [loaded, setLoaded] = useState<{ uuid: string; image: HTMLImageElement } | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (uuid === null) {
      return;
    }
    let active = true;
    void loadSkin(uuid).then((image) => {
      if (active && image !== null) {
        setLoaded({ uuid, image });
      }
    });
    return () => {
      active = false;
    };
  }, [uuid]);

  const skin = uuid !== null && loaded?.uuid === uuid ? loaded.image : null;
  const pixels = Math.round(size * Math.max(2, typeof window === "undefined" ? 1 : window.devicePixelRatio));

  useEffect(() => {
    if (skin !== null && canvas.current !== null) {
      drawHead(canvas.current, skin);
    }
  }, [skin, pixels]);

  if (skin !== null) {
    return (
      <canvas
        ref={canvas}
        className="account-avatar head"
        width={pixels}
        height={pixels}
        style={{ width: size, height: size }}
        aria-hidden="true"
      />
    );
  }

  const style = {
    "--avatar-hue": name === null ? 0 : coverHue(name),
    width: size,
    height: size,
    fontSize: Math.round(size * 0.42),
  } as CSSProperties;

  return (
    <span
      className={`account-avatar${name === null ? " empty" : ""}`}
      style={style}
      aria-hidden="true"
    >
      {name === null ? <Person20Regular /> : (name.charAt(0).toUpperCase() || "?")}
    </span>
  );
}
