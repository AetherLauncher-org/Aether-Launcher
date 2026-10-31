import { Badge, Button, Switch, Text } from "@fluentui/react-components";
import {
  Delete20Regular,
  Globe24Regular,
  Image24Regular,
  Open16Regular,
  PuzzlePiece24Regular,
  Sparkle24Regular,
} from "@fluentui/react-icons";
import type { CSSProperties, ReactElement } from "react";
import type { ContentItem, ContentKind, IdentifiedContent } from "../ipc";
import { contentCopy, describeShape, displayName, formatBytes } from "../lib/instanceContent";
import { coverHue } from "../lib/instances";

type ContentCardsProps = {
  /** 模组、资源包、光影、存档；投影有自己的卡片（`SchematicCards`） */
  kind: Exclude<ContentKind, "schematics">;
  items: ContentItem[];
  /** Modrinth 认出来的文件，按磁盘上的文件名索引；没认出来的照常显示文件名 */
  identified?: ReadonlyMap<string, IdentifiedContent>;
  /** 游戏正在运行：不能开关、不能删除 */
  locked: boolean;
  onToggle: (item: ContentItem, enabled: boolean) => void;
  onRemove: (item: ContentItem) => void;
  onOpenProject?: (found: IdentifiedContent) => void;
};

const icons: Record<ContentCardsProps["kind"], ReactElement> = {
  mods: <PuzzlePiece24Regular />,
  resourcepacks: <Image24Regular />,
  shaderpacks: <Sparkle24Regular />,
  saves: <Globe24Regular />,
};

/**
 * 模组、资源包、光影和存档的卡片网格，和投影一个样子：封面、名称、形态和大小，模组多一个
 * 开关，每张卡都能删除。
 *
 * 文件被 Modrinth 认出来时，封面换成项目图标，名称换成项目名，下面多出版本、简介和
 * 「在 Modrinth 打开」；磁盘上的文件名退到小字。没认出来的（本地改过的、不在 Modrinth
 * 上的）保持原样。名字只用来显示，传给后端的永远是磁盘上的纯文件名（见
 * `InstanceContentTab`），按钮的无障碍名称也始终用文件名。
 */
export function ContentCards({
  kind,
  items,
  identified,
  locked,
  onToggle,
  onRemove,
  onOpenProject,
}: ContentCardsProps) {
  const copy = contentCopy[kind];
  return (
    <ul className="content-grid">
      {items.map((item) => {
        const fileLabel = displayName(kind, item);
        const found = identified?.get(item.name);
        const title = found?.title ?? fileLabel;
        const off = kind === "mods" && !item.enabled;
        return (
          <li className="content-card" key={item.name} data-off={off} data-identified={found !== undefined}>
            <div
              className="content-cover"
              style={{ "--cover-hue": coverHue(title) } as CSSProperties}
              aria-hidden="true"
            >
              {found?.icon ? <img src={found.icon} alt="" draggable={false} /> : icons[kind]}
            </div>
            <div className="content-card-body">
              <div className="content-card-title">
                <Text weight="semibold" truncate wrap={false} title={found ? `${found.title} (${item.name})` : item.name}>
                  {title}
                </Text>
                {found ? (
                  <Text size={200} className="muted mono" truncate wrap={false}>
                    {fileLabel}
                  </Text>
                ) : null}
              </div>
              {found && found.summary ? (
                <Text size={200} className="muted content-card-summary">
                  {found.summary}
                </Text>
              ) : null}
              <div className="content-card-facts">
                <Badge appearance="tint" color="informative">
                  {describeShape(kind, item)}
                </Badge>
                {found && found.version ? (
                  <Badge appearance="tint" color="brand">
                    {found.version}
                  </Badge>
                ) : null}
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
                    aria-label={`启用 ${fileLabel}`}
                    checked={item.enabled}
                    disabled={locked}
                    onChange={(_, data) => onToggle(item, data.checked)}
                  />
                ) : (
                  <span />
                )}
                <div className="content-card-buttons">
                  {found && onOpenProject ? (
                    <Button
                      size="small"
                      appearance="subtle"
                      icon={<Open16Regular />}
                      aria-label={`在 Modrinth 打开 ${found.title}`}
                      onClick={() => onOpenProject(found)}
                    >
                      Modrinth
                    </Button>
                  ) : null}
                  <Button
                    size="small"
                    appearance="subtle"
                    icon={<Delete20Regular />}
                    aria-label={`删除 ${fileLabel}`}
                    title={`删除这个${copy.noun}`}
                    disabled={locked}
                    onClick={() => onRemove(item)}
                  />
                </div>
              </div>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
