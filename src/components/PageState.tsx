import {
  Button,
  MessageBar,
  MessageBarBody,
  Skeleton,
  SkeletonItem,
  Text,
} from "@fluentui/react-components";
import { ArrowClockwise24Regular } from "@fluentui/react-icons";

/** 列表加载时的骨架：几行等宽占位，形状和实例行一致。 */
export function ListSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div className="list-skeleton" aria-hidden="true">
      {Array.from({ length: rows }, (_, index) => (
        <div className="skeleton-row" key={index}>
          <Skeleton className="skeleton-thumb">
            <SkeletonItem shape="rectangle" size={40} />
          </Skeleton>
          <div className="skeleton-lines">
            <Skeleton>
              <SkeletonItem size={16} />
            </Skeleton>
            <Skeleton>
              <SkeletonItem size={12} />
            </Skeleton>
          </div>
          <Skeleton>
            <SkeletonItem shape="rectangle" size={56} />
          </Skeleton>
        </div>
      ))}
    </div>
  );
}

type ErrorStateProps = {
  title: string;
  message: string;
  onRetry?: () => void;
  retryLabel?: string;
};

/** 读取失败时的面板。错误原文放在更小的字里，方便对照日志。 */
export function ErrorState({
  title,
  message,
  onRetry,
  retryLabel = "重试",
}: ErrorStateProps) {
  return (
    <section className="status-panel error-panel" role="alert">
      <Text as="h2" size={400} weight="semibold">
        {title}
      </Text>
      <Text className="muted">{message}</Text>
      {onRetry ? (
        <Button
          appearance="secondary"
          icon={<ArrowClockwise24Regular />}
          onClick={onRetry}
        >
          {retryLabel}
        </Button>
      ) : null}
    </section>
  );
}

/**
 * 只在浏览器预览里出现的提示条：告诉用户这里需要桌面版。
 * 用 `MessageBar` 而不是错误面板——这不是故障，是运行环境不同。
 */
export function DesktopOnlyNotice({ body }: { body: string }) {
  return (
    <MessageBar intent="warning" className="desktop-notice">
      <MessageBarBody>{body}</MessageBarBody>
    </MessageBar>
  );
}
