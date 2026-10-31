import { Badge, Button, Switch, Text } from "@fluentui/react-components";
import {
  Delete20Regular,
  Globe24Regular,
  Image24Regular,
  PuzzlePiece24Regular,
  Sparkle24Regular,
} from "@fluentui/react-icons";
import type { CSSProperties, ReactElement } from "react";
import type { ContentItem, ContentKind } from "../ipc";
import { contentCopy, describeShape, displayName, formatBytes } from "../lib/instanceContent";
import { coverHue } from "../lib/instances";

type ContentCardsProps = {
  /** 模组、资源包、光影、存档；投影有自己的卡片（`SchematicCards`） */
  kind: Exclude<ContentKind, "schematics">;
  items: ContentItem[];
  /** 游戏正在运行：不能开关、不能删除 */
  locked: boolean;
  onToggle: (item: ContentItem, enabled: boolean) => void;
  onRemove: (item: ContentItem) => void;
};

const icons: Record<ContentCardsProps["kind"], ReactElement> = {
  mods: <PuzzlePiece24Regular />,
  resourcepacks: <Image24Regular />,
  shaderpacks: <Sparkle24Regular />,
  saves: <Globe24Regular />,
};

/**
 * 模组、资源包、光影和存档的卡片网格，和投影一个样子：一块由名字算出颜色的封面、名称、
 * 形态和大小，模组多一个开关，每张卡都能删除。名字只用来显示，传给后端的永远是磁盘上的
 * 纯文件名（见 `InstanceContentTab`）。
 */
export function ContentCards({ kind, items, locked, onToggle, onRemove }: ContentCardsProps) {
  const copy = contentCopy[kind];
  return (
    <ul className="content-grid">
      {items.map((item) => {
        const label = displayName(kind, item);
        const off = kind === "mods" && !item.enabled;
        return (
          <li className="content-card" key={item.name} data-off={off}>
            <div className="content-cover" style={{ "--cover-hue": coverHue(label) } as CSSProperties} aria-hidden="true">
              {icons[kind]}
            </div>
            <div className="content-card-body">
              <Text weight="semibold" truncate wrap={false} title={item.name}>
                {label}
              </Text>
              <div className="content-card-facts">
                <Badge appearance="tint" color="informative">
                  {describeShape(kind, item)}
                </Badge>
                {off ? (
                  <Badge appearance="tint" color="subtle">
                    已关闭
                  </Badge>
                ) : null}
                <Text size={200} className="muted">
                  {formatBytes(item.sizeBytes)}
                </Text>
              </div>
              <div className="content-card-actions">
                {kind === "mods" ? (
                  <Switch
                    aria-label={`启用 ${label}`}
                    checked={item.enabled}
                    disabled={locked}
                    onChange={(_, data) => onToggle(item, data.checked)}
                  />
                ) : (
                  <span />
                )}
                <Button
                  size="small"
                  appearance="subtle"
                  icon={<Delete20Regular />}
                  aria-label={`删除 ${label}`}
                  title={`删除这个${copy.noun}`}
                  disabled={locked}
                  onClick={() => onRemove(item)}
                />
              </div>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
