import { Text } from "@fluentui/react-components";
import type { ReactNode } from "react";

type PageHeaderProps = {
  title: string;
  description?: string;
  action?: ReactNode;
};

/** 页头：Fluent 的 Title 1 字号，下面一行说明，右侧放主操作。 */
export function PageHeader({ title, description, action }: PageHeaderProps) {
  return (
    <header className="page-header">
      <div className="page-header-copy">
        <Text as="h1" size={800} weight="semibold" className="page-title">
          {title}
        </Text>
        {description ? <Text className="page-subtitle">{description}</Text> : null}
      </div>
      {action ? <div className="page-header-action">{action}</div> : null}
    </header>
  );
}
