import { Person20Regular } from "@fluentui/react-icons";
import type { CSSProperties } from "react";
import { coverHue } from "../lib/instances";

type AccountAvatarProps = {
  /** 账户的玩家名；没有账户时传 null，显示一个中性的人像 */
  name: string | null;
  size?: number;
};

/**
 * 账户头像：玩家名首字母加一个由名字决定的色相，同一个名字永远是同一种颜色，
 * 和实例封面用同一套规则。离线账户没有皮肤可取，所以不假装有头像。
 */
export function AccountAvatar({ name, size = 40 }: AccountAvatarProps) {
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
