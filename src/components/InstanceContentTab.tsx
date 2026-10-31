import { Button, MessageBar, MessageBarBody, Spinner, Switch, Text } from "@fluentui/react-components";
import { FolderOpen20Regular } from "@fluentui/react-icons";
import { useCallback, useMemo, useState } from "react";
import {
  describeCommandError,
  identifyContent,
  isLookupKind,
  listInstanceContent,
  openContentProject,
  openInstanceFolder,
  removeContent,
  setModEnabled,
} from "../ipc";
import type { ContentItem, ContentKind, ContentLookup, IdentifiedContent } from "../ipc";
import { loadContentLookup, saveContentLookup } from "../lib/contentLookup";
import { contentCopy, describeRemoval } from "../lib/instanceContent";
import { useQuery } from "../lib/query";
import { previews } from "../lib/schematic/previewQueue";
import { ConfirmDialog } from "./ConfirmDialog";
import { ContentCards } from "./ContentCards";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./PageState";
import { RefreshButton } from "./RefreshButton";
import { SchematicCards } from "./SchematicCards";
import { WorldDialog } from "./WorldDialog";

type InstanceContentTabProps = {
  instanceId: string;
  kind: ContentKind;
  /** 游戏正在运行：不能改文件（后端也会拒绝），界面上直接灰掉并说明 */
  locked: boolean;
  /** 列表有变化（删除、开关）后调用，让详情里的计数跟着更新 */
  onChanged: () => void;
};

/**
 * 一种内容的列表：模组、资源包、光影、存档、投影共用。
 *
 * 每一项只有名字、大小，模组多一个开关（`.jar` ↔ `.jar.disabled`）。删除要确认，
 * 世界会写明连同里面的一切一起删。名字只是显示用，传给后端的永远是磁盘上的纯文件名。
 */
export function InstanceContentTab({ instanceId, kind, locked, onChanged }: InstanceContentTabProps) {
  const copy = contentCopy[kind];
  const load = useCallback(() => listInstanceContent(instanceId, kind), [instanceId, kind]);
  const { status, data, error, reload } = useQuery(load);

  const [notice, setNotice] = useState<string | null>(null);
  const [removing, setRemoving] = useState<ContentItem | null>(null);
  const [world, setWorld] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [removeError, setRemoveError] = useState<string | null>(null);

  const items = data ?? [];

  // 名称、简介和图标来自 Modrinth，在列表出来之后另外取：列表不等网络，识别失败也不影响列表。
  const lookupKind = isLookupKind(kind) ? kind : null;
  const [lookupOn, setLookupOn] = useState(loadContentLookup);
  const loadIdentity = useCallback(
    (): Promise<ContentLookup> =>
      lookupKind === null
        ? Promise.resolve({ items: [], unreachable: false })
        : identifyContent(instanceId, lookupKind),
    [instanceId, lookupKind],
  );
  const identity = useQuery(loadIdentity, lookupKind !== null && lookupOn && items.length > 0);
  const identified = useMemo<ReadonlyMap<string, IdentifiedContent> | undefined>(
    () =>
      lookupOn && identity.data !== null
        ? new Map(identity.data.items.map((found) => [found.name, found]))
        : undefined,
    [lookupOn, identity.data],
  );
  const reloadAll = () => {
    reload();
    identity.reload();
  };

  const openProject = async (found: IdentifiedContent) => {
    setNotice(null);
    try {
      await openContentProject(found.projectSlug);
    } catch (failure: unknown) {
      setNotice(describeCommandError(failure));
    }
  };

  const toggle = async (item: ContentItem, enabled: boolean) => {
    setNotice(null);
    try {
      await setModEnabled(instanceId, item.name, enabled);
      reload();
      onChanged();
    } catch (failure: unknown) {
      setNotice(describeCommandError(failure));
    }
  };

  const confirmRemove = async () => {
    if (removing === null) {
      return;
    }
    setBusy(true);
    setRemoveError(null);
    try {
      await removeContent(instanceId, kind, removing.name);
      previews.forget(`${instanceId}/${removing.name}/`);
      setRemoving(null);
      reload();
      onChanged();
    } catch (failure: unknown) {
      setRemoveError(describeCommandError(failure));
    } finally {
      setBusy(false);
    }
  };

  const openFolder = async () => {
    setNotice(null);
    try {
      await openInstanceFolder(instanceId, kind);
    } catch (failure: unknown) {
      setNotice(describeCommandError(failure));
    }
  };

  return (
    <section className="manage-panel" aria-label={copy.tab}>
      <div className="toolbar">
        <Text className="muted">{status === "success" ? `共 ${items.length} 项` : " "}</Text>
        <div className="toolbar-actions">
          {lookupKind !== null ? (
            <Switch
              label="联网识别"
              title="到 Modrinth 按文件的哈希识别名称、简介和图标。只发送哈希，不发送文件名。"
              checked={lookupOn}
              onChange={(_, switched) => {
                setLookupOn(switched.checked);
                saveContentLookup(switched.checked);
              }}
            />
          ) : null}
          <Button appearance="subtle" icon={<FolderOpen20Regular />} onClick={() => void openFolder()}>
            {copy.openLabel}
          </Button>
          <RefreshButton label={`刷新${copy.tab}列表`} loading={status === "loading"} onClick={reloadAll} />
        </div>
      </div>
      {lookupKind !== null && lookupOn && identity.status === "loading" && identity.data === null ? (
        <Text size={200} className="muted" role="status">
          正在到 Modrinth 识别…
        </Text>
      ) : null}
      {identity.data?.unreachable && lookupOn ? (
        <Text size={200} className="muted" role="status">
          没连上 Modrinth，显示的是上次识别的结果。
        </Text>
      ) : null}

      {locked ? (
        <MessageBar intent="info" className="page-notice">
          <MessageBarBody>游戏正在运行，停止游戏之后才能修改这里的文件。</MessageBarBody>
        </MessageBar>
      ) : null}
      {notice ? (
        <MessageBar intent="error" className="page-notice" role="alert">
          <MessageBarBody>{notice}</MessageBarBody>
        </MessageBar>
      ) : null}

      {status === "error" && error ? (
        <ErrorState title={`读取${copy.tab}失败`} message={error} onRetry={reload} retryLabel="重新读取" />
      ) : status === "loading" && data === null ? (
        <div className="loading-block" role="status">
          <Spinner label={`正在读取${copy.tab}…`} />
        </div>
      ) : items.length === 0 ? (
        <EmptyState
          title={copy.emptyTitle}
          body={copy.emptyBody}
          action={
            <Button appearance="secondary" icon={<FolderOpen20Regular />} onClick={() => void openFolder()}>
              {copy.openLabel}
            </Button>
          }
        />
      ) : kind === "schematics" ? (
        <SchematicCards
          instanceId={instanceId}
          items={items}
          locked={locked}
          onRemove={(item) => {
            setRemoveError(null);
            setRemoving(item);
          }}
          onConverted={() => {
            reload();
            onChanged();
          }}
        />
      ) : (
        <ContentCards
          kind={kind}
          items={items}
          identified={identified}
          locked={locked}
          onOpenProject={(found) => void openProject(found)}
          onOpenWorld={(item) => setWorld(item.name)}
          onToggle={(item, enabled) => void toggle(item, enabled)}
          onRemove={(item) => {
            setRemoveError(null);
            setRemoving(item);
          }}
        />
      )}

      {kind === "saves" ? (
        <WorldDialog
          instanceId={instanceId}
          world={world}
          locked={locked}
          onClose={() => setWorld(null)}
          onChanged={onChanged}
        />
      ) : null}

      <ConfirmDialog
        open={removing !== null}
        title={`删除这个${copy.noun}？`}
        body={removing === null ? null : <Text>{describeRemoval(kind, removing)}</Text>}
        confirmLabel="删除"
        danger
        busy={busy}
        error={removeError}
        onConfirm={() => void confirmRemove()}
        onCancel={() => setRemoving(null)}
      />
    </section>
  );
}
