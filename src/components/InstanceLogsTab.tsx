import { Button, MessageBar, MessageBarBody, Select, Spinner, Text } from "@fluentui/react-components";
import { ArrowSync20Regular, Copy20Regular, FolderOpen20Regular } from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import { describeCommandError, listInstanceLogs, openInstanceFolder, readInstanceLog } from "../ipc";
import type { LogEntry } from "../ipc";
import { describeLog, formatBytes } from "../lib/instanceContent";
import { useQuery } from "../lib/query";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./PageState";

type InstanceLogsTabProps = {
  instanceId: string;
};

const entryKey = (entry: LogEntry) => `${entry.kind}:${entry.name}`;

/**
 * 日志和崩溃报告。
 *
 * 文本在后端已经脱敏（玩家名、令牌、邮箱、IP、用户目录），这里原样显示。最多读最后
 * 256 KiB，太长的日志会说明开头被截掉了。想把日志发给别人时，复制出来的就是脱敏后的文本。
 */
export function InstanceLogsTab({ instanceId }: InstanceLogsTabProps) {
  const loadList = useCallback(() => listInstanceLogs(instanceId), [instanceId]);
  const list = useQuery(loadList);
  const entries = list.data ?? [];

  // undefined：用户还没选，显示第一项
  const [chosenKey, setChosenKey] = useState<string | undefined>(undefined);
  const selected = entries.find((entry) => entryKey(entry) === chosenKey) ?? entries[0] ?? null;

  const loadText = useCallback(
    () => (selected === null ? Promise.reject(new Error("没有日志")) : readInstanceLog(instanceId, selected)),
    [instanceId, selected],
  );
  const text = useQuery(loadText, selected !== null);

  const [notice, setNotice] = useState<string | null>(null);

  const copy = async () => {
    if (text.data === null) {
      return;
    }
    try {
      await navigator.clipboard.writeText(text.data.text);
      setNotice("已复制（内容已脱敏）");
    } catch {
      setNotice("无法复制，请手动选中日志文字");
    }
  };

  const openFolder = async () => {
    try {
      await openInstanceFolder(instanceId, selected?.kind === "crash" ? "crash-reports" : "logs");
      setNotice(null);
    } catch (failure: unknown) {
      setNotice(describeCommandError(failure));
    }
  };

  const refresh = () => {
    setNotice(null);
    list.reload();
    text.reload();
  };

  if (list.status === "error" && list.error) {
    return <ErrorState title="读取日志失败" message={list.error} onRetry={list.reload} retryLabel="重新读取" />;
  }
  if (list.status === "loading" && list.data === null) {
    return (
      <div className="loading-block" role="status">
        <Spinner label="正在读取日志…" />
      </div>
    );
  }
  if (entries.length === 0) {
    return (
      <EmptyState
        title="还没有日志"
        body="游戏运行过之后，日志和崩溃报告会出现在这里。"
        action={
          <Button appearance="secondary" icon={<FolderOpen20Regular />} onClick={() => void openFolder()}>
            打开日志文件夹
          </Button>
        }
      />
    );
  }

  return (
    <section className="manage-panel" aria-label="日志">
      <div className="toolbar">
        <Select
          className="log-select"
          aria-label="选择日志"
          value={selected === null ? "" : entryKey(selected)}
          onChange={(_, data) => setChosenKey(data.value)}
        >
          {entries.map((entry) => (
            <option key={entryKey(entry)} value={entryKey(entry)}>
              {describeLog(entry)} · {formatBytes(entry.sizeBytes)}
            </option>
          ))}
        </Select>
        <div className="toolbar-actions">
          <Button appearance="subtle" icon={<Copy20Regular />} disabled={text.data === null} onClick={() => void copy()}>
            复制
          </Button>
          <Button appearance="subtle" icon={<FolderOpen20Regular />} onClick={() => void openFolder()}>
            打开文件夹
          </Button>
          <Button appearance="subtle" icon={<ArrowSync20Regular />} aria-label="刷新日志" onClick={refresh} />
        </div>
      </div>

      {notice ? (
        <Text size={200} role="status" className="muted">
          {notice}
        </Text>
      ) : null}
      {text.data?.truncated ? (
        <MessageBar intent="info" className="page-notice">
          <MessageBarBody>日志很长，只显示最后 256 KB。</MessageBarBody>
        </MessageBar>
      ) : null}

      {text.status === "error" && text.error ? (
        <MessageBar intent="error" className="page-notice" role="alert">
          <MessageBarBody>读取这份日志失败：{text.error}</MessageBarBody>
        </MessageBar>
      ) : text.data === null ? (
        <div className="loading-block" role="status">
          <Spinner label="正在读取…" />
        </div>
      ) : (
        <pre className="log-view" tabIndex={0} aria-label="日志内容">
          {text.data.text === "" ? "（这份日志是空的）" : text.data.text}
        </pre>
      )}
      <Text size={200} className="muted">
        玩家名、令牌、邮箱、IP 地址和用户目录已被替换成占位符。
      </Text>
    </section>
  );
}
