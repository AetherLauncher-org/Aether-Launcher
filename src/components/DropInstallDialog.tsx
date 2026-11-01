import {
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  MessageBar,
  MessageBarBody,
  Select,
  Spinner,
  Text,
} from "@fluentui/react-components";
import { useCallback, useEffect, useState } from "react";
import { describeCommandError, listInstanceContent } from "../ipc";
import { installDrop, listenDrop } from "../ipc/drop";
import type { DropPreview, DropReport, DroppedItem } from "../ipc/drop";
import { formatBytes } from "../lib/instanceContent";
import { useQuery } from "../lib/query";
import { useInstances } from "../state/instances";

const kindLabels: Record<DroppedItem["kind"], string> = {
  mod: "模组",
  datapack: "数据包",
  resourcepack: "资源包",
  shader: "光影包",
};

/**
 * 拖放安装的确认对话框。后端把拖进窗口的文件收在令牌后面：这里只拿到
 * 名字、大小和类型，没有任何路径；确认时把令牌和目标交给后端，由它
 * 重新校验并复制。每个文件各自成功或跳过，结果逐项列出。
 */
export function DropInstallDialog() {
  const { instances } = useInstances();
  const [preview, setPreview] = useState<DropPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<DropReport | null>(null);
  const [chosenId, setChosenId] = useState<string | null>(null);
  const [worlds, setWorlds] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);

  // 没选过就用第一个实例；选的那个被删了也回到第一个
  const current = instances.find((instance) => instance.id === chosenId) ?? instances[0] ?? null;
  const hasDatapacks = preview?.items.some((item) => item.kind === "datapack") ?? false;

  const loadWorlds = useCallback(
    () => listInstanceContent(current?.id ?? "", "saves"),
    [current?.id],
  );
  const saves = useQuery(loadWorlds, preview !== null && hasDatapacks && current !== null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenDrop(
      (next) => {
        setPreview(next);
        setWorlds(new Set());
        setReport(null);
        setError(null);
      },
      (message) => {
        setError(message);
        setPreview(null);
        setReport(null);
      },
    )
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const close = () => {
    if (!busy) {
      setPreview(null);
      setReport(null);
      setError(null);
    }
  };

  const install = async () => {
    if (preview === null || current === null) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setReport(await installDrop(preview.token, current.id, [...worlds]));
      setPreview(null);
    } catch (cause) {
      setError(describeCommandError(cause));
    } finally {
      setBusy(false);
    }
  };

  const open = preview !== null || report !== null || error !== null;

  return (
    <Dialog open={open} onOpenChange={(_, data) => { if (!data.open) close(); }}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>
            {report !== null ? "安装结果" : preview !== null ? "安装拖入的文件" : "无法处理拖入的文件"}
          </DialogTitle>
          <DialogContent>
            {error !== null && (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{error}</MessageBarBody>
              </MessageBar>
            )}
            {preview !== null && (
              <>
                <Text as="p">确认文件和目标后安装；同名文件不会覆盖。</Text>
                <ul>
                  {preview.items.map((item, index) => (
                    <li key={`${item.name}-${index}`}>
                      {item.name} · {kindLabels[item.kind]} · {formatBytes(item.size)}
                    </li>
                  ))}
                </ul>
                <label htmlFor="drop-instance">安装到实例</label>
                <Select
                  id="drop-instance"
                  value={current?.id ?? ""}
                  disabled={busy || current === null}
                  onChange={(_, data) => {
                    setChosenId(data.value);
                    setWorlds(new Set());
                  }}
                >
                  {instances.map((instance) => (
                    <option key={instance.id} value={instance.id}>
                      {instance.name}
                    </option>
                  ))}
                </Select>
                {instances.length === 0 && <Text as="p">还没有实例，请先创建一个实例。</Text>}
                {hasDatapacks && (
                  <>
                    <Text as="p">数据包装进哪些世界？</Text>
                    {saves.status === "error" ? (
                      <Text as="p">{saves.error}</Text>
                    ) : saves.data === null ? (
                      <Spinner label="正在读取世界…" />
                    ) : saves.data.length === 0 ? (
                      <Text as="p">这个实例还没有世界，先启动游戏创建一个。</Text>
                    ) : (
                      saves.data.map((world) => (
                        <Checkbox
                          key={world.name}
                          label={world.name}
                          checked={worlds.has(world.name)}
                          onChange={(_, data) =>
                            setWorlds((previous) => {
                              const next = new Set(previous);
                              if (data.checked === true) {
                                next.add(world.name);
                              } else {
                                next.delete(world.name);
                              }
                              return next;
                            })
                          }
                        />
                      ))
                    )}
                  </>
                )}
              </>
            )}
            {report !== null && (
              <>
                <Text as="p">
                  已安装 {report.installed.length} 项，跳过 {report.skipped.length} 项。
                </Text>
                <ul>
                  {report.installed.map((item, index) => (
                    <li key={`ok-${index}`}>
                      {item.name}
                      {item.world === null ? "" : ` · ${item.world}`} — 已安装
                    </li>
                  ))}
                  {report.skipped.map((item, index) => (
                    <li key={`skip-${index}`}>
                      {item.name}
                      {item.world === null ? "" : ` · ${item.world}`} — {item.reason}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </DialogContent>
          <DialogActions>
            <Button onClick={close} disabled={busy}>
              {preview !== null ? "取消" : "关闭"}
            </Button>
            {preview !== null && (
              <Button
                appearance="primary"
                disabled={busy || current === null || (hasDatapacks && worlds.size === 0)}
                onClick={() => void install()}
              >
                {busy ? "正在安装…" : "安装"}
              </Button>
            )}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
