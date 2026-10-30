import { Button } from "@fluentui/react-components";
import { ArrowSync20Regular } from "@fluentui/react-icons";
import { useEffect, useState } from "react";

type RefreshButtonProps = {
  /** 读屏器和提示用的名字，例如「刷新模组列表」 */
  label: string;
  /** 正在重新读取：图标转，按钮暂时不能再点 */
  loading: boolean;
  onClick: () => void;
};

/** 点击后至少转一整圈，本地读取太快时也看得出「确实刷新了」。 */
const MIN_SPIN_MS = 700;

/** 刷新按钮：读取期间图标转动，读得再快也至少转一圈。 */
export function RefreshButton({ label, loading, onClick }: RefreshButtonProps) {
  const [pressed, setPressed] = useState(false);

  useEffect(() => {
    if (!pressed) {
      return;
    }
    const timer = window.setTimeout(() => setPressed(false), MIN_SPIN_MS);
    return () => window.clearTimeout(timer);
  }, [pressed]);

  const spinning = loading || pressed;

  return (
    <Button
      appearance="subtle"
      aria-label={label}
      aria-busy={spinning}
      disabled={loading}
      icon={<ArrowSync20Regular className={spinning ? "spin-icon" : undefined} />}
      onClick={() => {
        setPressed(true);
        onClick();
      }}
    />
  );
}
