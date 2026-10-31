import {
  Badge,
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  MessageBar,
  MessageBarBody,
  Spinner,
  Text,
} from "@fluentui/react-components";
import { Delete20Regular, FolderOpen20Regular, Globe24Regular } from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import { describeCommandError, getWorldDetails, openWorldFolder, removeWorldDatapack } from "../ipc";
import type { DatapackInfo } from "../ipc";
import { formatBytes, formatModified } from "../lib/instanceContent";
import { useQuery } from "../lib/query";
import { describeDifficulty, describeMode, describePackRemoval, packStateLabels } from "../lib/world";
import { ConfirmDialog } from "./ConfirmDialog";

type WorldDialogProps = {
  instanceId: string;
  /** `saves/` 里的文件夹名；null 时对话框关着 */
  world: string | null;
  /** 游戏正在运行：不能删数据包（后端也会拒绝） */
  locked: boolean;
  onClose: () => void;
  /** 数据包删除之后调用 */
  onChanged: () => void;
};

/**
 * 一个世界的详情：`level.dat` 里的基本信息，和 `datapacks/` 里每个数据包的名字、大小、
 * 描述、pack_format 和启用状态。能删数据包、打开世界文件夹；启用／停用要改 `level.dat`，
 * 这里不做。
 *
 * `level.dat` 和 `pack.mcmeta` 是不可信输入，后端已限长清洗；这里仍然只把它们当纯文本显示。
 */
export function WorldDialog({ instanceId, world, locked, onClose, onChanged }: WorldDialogProps) {
  const open = world !== null;
  const load = useCallback(
    () => getWorldDetails(instanceId, world ?? ""),
    [instanceId, world],
  );
  const { status, data, error, reload } = useQuery(load, open);

  const [notice, setNotice] = useState<string | null>(null);
  const [removing, setRemoving] = useState<DatapackInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [removeError, setRemoveError] = useState<string | null>(null);

  const confirmRemove = async () => {
    if (removing === null || world === null) {
      return;
    }
    setBusy(true);
    setRemoveError(null);
    try {
      await removeWorldDatapack(instanceId, world, removing.name);
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
    if (world === null) {
      return;
    }
    setNotice(null);
    try {
      await openWorldFolder(instanceId, world);
    } catch (failure: unknown) {
      setNotice(describeCommandError(failure));
    }
  };

  const facts: [string, string | null][] =
    data === null
      ? []
      : [
          ["游戏版本", data.gameVersion],
          ["模式", describeMode(data)],
          ["难度", describeDifficulty(data.difficulty)],
          ["上次游玩", data.lastPlayed === null ? null : formatModified(data.lastPlayed)],
          ["大小", formatBytes(data.sizeBytes)],
          ["DataVersion", data.dataVersion === null ? null : String(data.dataVersion)],
        ];

  return (
    <Dialog
      open={open}
      onOpenChange={(_, next) => {
        if (!next.open) {
          setNotice(null);
          onClose();
        }
      }}
    >
      <DialogSurface className="world-dialog">
        <DialogBody>
          <DialogTitle>{data?.levelName ?? world ?? ""}</DialogTitle>
          <DialogContent className="dialog-content">
            {status === "error" && error ? (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{error}</MessageBarBody>
              </MessageBar>
            ) : data === null ? (
              <div className="loading-block" role="status">
                <Spinner label="正在读取世界…" />
              </div>
            ) : (
              <>
                <div className="world-head">
                  <div className="world-icon" aria-hidden="true">
                    {data.icon ? <img src={data.icon} alt="" draggable={false} /> : <Globe24Regular />}
                  </div>
                  <dl className="world-facts">
                    <dt>文件夹</dt>
                    <dd className="mono">{data.name}</dd>
                    {facts.map(([label, value]) =>
                      value === null || value === "" ? null : (
                        <div key={label} className="world-fact">
                          <dt>{label}</dt>
                          <dd>{value}</dd>
                        </div>
                      ),
                    )}
                  </dl>
                </div>
                {data.unreadable ? (
                  <MessageBar intent="warning" role="status">
                    <MessageBarBody>没能读懂这个世界的 level.dat（{data.unreadable}），上面的信息不全。</MessageBarBody>
                  </MessageBar>
                ) : null}

                <Text weight="semibold" as="h3" className="world-section">
                  数据包（{data.datapacks.length}
                  {data.datapacksTruncated ? "+" : ""}）
                </Text>
                {data.datapacks.length === 0 ? (
                  <Text className="muted">这个世界没有数据包。</Text>
                ) : (
                  <ul className="datapack-list">
                    {data.datapacks.map((pack) => {
                      const state = packStateLabels[pack.state];
                      return (
                        <li key={pack.name} className="datapack-row">
                          <div className="datapack-main">
                            <Text weight="semibold" truncate wrap={false} title={pack.name}>
                              {pack.name}
                            </Text>
                            {pack.description ? (
                              <Text size={200} className="muted datapack-description">
                                {pack.description}
                              </Text>
                            ) : null}
                            <div className="content-card-facts">
                              <Badge
                                appearance="tint"
                                color={pack.state === "enabled" ? "success" : pack.state === "disabled" ? "subtle" : "informative"}
                                title={state.hint}
                              >
                                {state.text}
                              </Badge>
                              <Badge appearance="tint" color="informative">
                                {pack.isFolder ? "文件夹" : "压缩包"}
                              </Badge>
                              {pack.packFormat !== null ? (
                                <Text size={200} className="muted">
                                  pack_format {pack.packFormat}
                                </Text>
                              ) : null}
                              <Text size={200} className="muted">
                                {formatBytes(pack.sizeBytes)}
                              </Text>
                            </div>
                          </div>
                          <Button
                            size="small"
                            appearance="subtle"
                            icon={<Delete20Regular />}
                            aria-label={`删除数据包 ${pack.name}`}
                            title={locked ? "游戏正在运行，停止后才能删除" : "删除这个数据包"}
                            disabled={locked}
                            onClick={() => {
                              setRemoveError(null);
                              setRemoving(pack);
                            }}
                          />
                        </li>
                      );
                    })}
                  </ul>
                )}
                {data.datapacksTruncated ? (
                  <Text size={200} className="muted">
                    数据包太多，只列出了前面一部分。
                  </Text>
                ) : null}
              </>
            )}
            {notice ? (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{notice}</MessageBarBody>
              </MessageBar>
            ) : null}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" icon={<FolderOpen20Regular />} onClick={() => void openFolder()}>
              打开世界文件夹
            </Button>
            <Button appearance="primary" onClick={onClose}>
              关闭
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>

      <ConfirmDialog
        open={removing !== null}
        title="删除这个数据包？"
        body={removing === null ? null : <Text>{describePackRemoval(removing.name, removing.isFolder)}</Text>}
        confirmLabel="删除"
        danger
        busy={busy}
        error={removeError}
        onConfirm={() => void confirmRemove()}
        onCancel={() => setRemoving(null)}
      />
    </Dialog>
  );
}
