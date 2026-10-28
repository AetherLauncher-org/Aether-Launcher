import type { ReactNode } from "react";

type PageTransitionProps = {
  /** 页面标识。变化时重新挂载容器，让进入动画再播放一次。 */
  pageKey: string;
  children: ReactNode;
};

/**
 * 页面切换动画。用页面标识做 `key`：切换页面时容器被替换，CSS 里的 `page-enter`
 * 关键帧会从头播放（淡入并从下方滑入）。动画的时长和曲线取自 Fluent 的动效令牌，
 * 用户关闭动画或系统开启「减少动态效果」时不播放（见 `motion.css`）。
 */
export function PageTransition({ pageKey, children }: PageTransitionProps) {
  return (
    <div className="page" key={pageKey} data-page={pageKey}>
      {children}
    </div>
  );
}
