import { useEffect } from "react";

import { useNavigation } from "../state/navigation";
import type { PageId } from "../navigation";

type AutoReturnProps = {
  /** 安装结束后回到哪一页。 */
  to: PageId;
  /** 跳转前顺手清理状态（例如把已结束的安装记录 dismiss 掉）。 */
  onLeaving?: () => void;
};

/**
 * 安装结束后自动跳页。
 *
 * 装完就该能启动了，留在详情页只会让人以为卡住了。渲染期间不做副作用：
 * 跳转放进 effect 里，且 `onLeaving` 只跑一次。
 */
export function AutoReturn({ to, onLeaving }: AutoReturnProps) {
  const { navigate } = useNavigation();

  useEffect(() => {
    onLeaving?.();
    navigate(to);
  }, [navigate, onLeaving, to]);

  // 跳转前的这一帧不显示内容，避免闪一下空白成功卡片。
  return null;
}
