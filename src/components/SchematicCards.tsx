import { Badge, Button, MessageBar, MessageBarBody, Spinner, Text } from "@fluentui/react-components";
import { ArrowSwap20Regular, Cube20Regular, Delete20Regular } from "@fluentui/react-icons";
import { Suspense, lazy, useEffect, useState } from "react";
import { describeCommandError, readInstanceSchematic } from "../ipc";
import type { ContentItem, ConversionResult, SchematicFormat, SchematicView } from "../ipc";
import { formatBytes } from "../lib/instanceContent";
import { previews } from "../lib/schematic/previewQueue";
import type { Preview } from "../lib/schematic/previewQueue";
import { ConvertSchematicDialog } from "./ConvertSchematicDialog";

// three.js 只在真的要看 3D 的时候才下载和解析。
const SchematicViewer = lazy(() => import("./SchematicViewer").then((module) => ({ default: module.SchematicViewer })));

const FORMAT_NAMES: Record<SchematicFormat, string> = {
  litematic: "Litematica",
  schem: "Sponge",
  nbt: "结构文件",
};

type SchematicCardsProps = {
  instanceId: string;
  items: ContentItem[];
  /** 游戏正在运行：不能删除、转换 */
  locked: boolean;
  onRemove: (item: ContentItem) => void;
  /** 转换产生了新文件，列表需要重新读取 */
  onConverted: (result: ConversionResult) => void;
};

/** 缓存的键带上文件大小，同名文件被改过就会重新读。 */
const previewKey = (instanceId: string, item: ContentItem) => `${instanceId}/${item.name}/${item.sizeBytes}`;

/**
 * 「投影」标签页的卡片：每个文件一张，带缩略图、格式、尺寸、方块数、数据版本，
 * 可以进 3D 预览、转换、删除。缩略图一张一张地读和画（见 `previewQueue`），不会因为
 * 文件夹里有很多投影而卡住页面。
 */
export function SchematicCards({ instanceId, items, locked, onRemove, onConverted }: SchematicCardsProps) {
  const [viewing, setViewing] = useState<SchematicView | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [converting, setConverting] = useState<{ item: ContentItem; preview: Preview | null } | null>(null);
  const [previewsByName, setPreviewsByName] = useState<Record<string, Preview>>({});

  const open3D = async (item: ContentItem) => {
    setNotice(null);
    setOpening(item.name);
    try {
      setViewing(await readInstanceSchematic(instanceId, item.name));
    } catch (failure: unknown) {
      setNotice(`打开「${item.name}」失败：${describeCommandError(failure)}`);
    } finally {
      setOpening(null);
    }
  };

  return (
    <>
      {notice ? (
        <MessageBar intent="error" className="page-notice" role="alert">
          <MessageBarBody>{notice}</MessageBarBody>
        </MessageBar>
      ) : null}
      <ul className="schematic-grid">
        {items.map((item) => (
          <SchematicCard
            key={item.name}
            instanceId={instanceId}
            item={item}
            locked={locked}
            busy={opening === item.name}
            onPreview={(preview) => setPreviewsByName((current) => ({ ...current, [item.name]: preview }))}
            onOpen={() => void open3D(item)}
            onConvert={() => setConverting({ item, preview: previewsByName[item.name] ?? null })}
            onRemove={() => onRemove(item)}
          />
        ))}
      </ul>

      {viewing !== null ? (
        <Suspense
          fallback={
            <div className="viewer-overlay viewer-loading" role="status">
              <Spinner label="正在加载 3D 预览…" />
            </div>
          }
        >
          <SchematicViewer view={viewing} onClose={() => setViewing(null)} />
        </Suspense>
      ) : null}

      {converting !== null ? (
        <ConvertSchematicDialog
          open
          instanceId={instanceId}
          fileName={converting.item.name}
          currentFormat={converting.preview?.summary.format ?? null}
          currentDataVersion={converting.preview?.summary.dataVersion ?? null}
          onClose={() => setConverting(null)}
          onConverted={onConverted}
        />
      ) : null}
    </>
  );
}

type SchematicCardProps = {
  instanceId: string;
  item: ContentItem;
  locked: boolean;
  busy: boolean;
  onPreview: (preview: Preview) => void;
  onOpen: () => void;
  onConvert: () => void;
  onRemove: () => void;
};

function SchematicCard({ instanceId, item, locked, busy, onPreview, onOpen, onConvert, onRemove }: SchematicCardProps) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    previews
      .request(previewKey(instanceId, item), () => readInstanceSchematic(instanceId, item.name))
      .then((loaded) => {
        if (!cancelled) {
          setPreview(loaded);
          onPreview(loaded);
        }
      })
      .catch((failure: unknown) => {
        if (!cancelled) {
          setProblem(describeCommandError(failure));
        }
      });
    return () => {
      cancelled = true;
    };
    // `onPreview` 每次渲染都是新函数，只在文件变了才重新请求。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId, item.name, item.sizeBytes]);

  const summary = preview?.summary ?? null;
  const [sx, sy, sz] = summary?.size ?? [0, 0, 0];

  return (
    <li className="schematic-card">
      <button
        type="button"
        className="schematic-thumb"
        aria-label={`3D 预览 ${item.name}`}
        disabled={problem !== null || busy}
        onClick={onOpen}
      >
        {preview?.image ? (
          <img src={preview.image} alt="" />
        ) : problem !== null ? (
          <Text size={200} className="muted">
            读不出来
          </Text>
        ) : preview === null ? (
          <Spinner size="tiny" />
        ) : (
          <Cube20Regular aria-hidden="true" />
        )}
      </button>
      <div className="schematic-body">
        <Text weight="semibold" truncate wrap={false} title={item.name}>
          {summary !== null && summary.name !== "" ? summary.name : item.name}
        </Text>
        {problem !== null ? (
          <Text size={200} className="schematic-problem" role="alert">
            {problem}
          </Text>
        ) : (
          <div className="schematic-facts">
            {summary !== null ? (
              <>
                <Badge appearance="tint" color="brand">
                  {FORMAT_NAMES[summary.format]}
                </Badge>
                <Text size={200} className="muted">
                  {sx}×{sy}×{sz} · {summary.solidBlocks.toLocaleString()} 方块
                </Text>
                {summary.dataVersion !== null ? (
                  <Text size={200} className="muted">
                    数据版本 {summary.dataVersion}
                  </Text>
                ) : null}
              </>
            ) : (
              <Text size={200} className="muted">
                读取中…
              </Text>
            )}
            <Text size={200} className="muted">
              {formatBytes(item.sizeBytes)}
            </Text>
          </div>
        )}
        <div className="schematic-actions">
          <Button size="small" appearance="primary" icon={<Cube20Regular />} disabled={problem !== null || busy} onClick={onOpen}>
            {busy ? "正在打开…" : "3D 查看"}
          </Button>
          <Button size="small" icon={<ArrowSwap20Regular />} disabled={problem !== null || locked} onClick={onConvert}>
            转换…
          </Button>
          <Button
            size="small"
            appearance="subtle"
            icon={<Delete20Regular />}
            aria-label={`删除 ${item.name}`}
            disabled={locked}
            onClick={onRemove}
          />
        </div>
      </div>
    </li>
  );
}
