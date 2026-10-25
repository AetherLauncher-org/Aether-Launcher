import { Text } from "@fluentui/react-components";
import type { ReactNode } from "react";

type EmptyStateProps = {
  title: string;
  body: string;
  /** 可选的下一步操作，比如「创建实例」 */
  action?: ReactNode;
  /** 上方的小图标 */
  icon?: ReactNode;
};

/**
 * 空状态。三个页面共用一套排版：图标、标题、一句说明、一个可选动作。
 * 放在 `.status-panel` 里时由调用方决定外框，这里只管内容。
 */
export function EmptyState({ title, body, action, icon }: EmptyStateProps) {
  return (
    <div className="empty-state">
      {icon ? (
        <span className="empty-icon" aria-hidden="true">
          {icon}
        </span>
      ) : null}
      <Text as="h2" size={400} weight="semibold">
        {title}
      </Text>
      <Text className="muted empty-copy">{body}</Text>
      {action ? <div className="empty-action">{action}</div> : null}
    </div>
  );
}
