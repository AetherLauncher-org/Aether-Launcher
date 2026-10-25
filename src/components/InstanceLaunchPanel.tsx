import { Badge, Button, Input, Text } from "@fluentui/react-components";
import { useState } from "react";
import {
  activateInstalledInstance,
  describeCommandError,
  isTauri,
  startInstalledInstance,
  stopInstalledInstance,
} from "../ipc";
import type { GameRunStatus, InstanceState } from "../ipc";
import { validatePlayerName } from "../lib/accounts";
import {
  buttonLabel,
  canStart,
  canStop,
  describeExit,
  describePhase,
  initialLabel,
  launchPhase,
} from "../lib/launchState";
import { formatMemoryMiB } from "../lib/memory";
import { loadMemoryMiB } from "../lib/settings";

type InstanceLaunchPanelProps = {
  instanceId: string;
  /** 实例清单里的上次已知状态；面板在拿到任何命令结果前只显示「未验证」式标签 */
  initialState?: InstanceState;
  disabled?: boolean;
};

/**
 * 单个实例的启动面板：离线玩家名、内存、状态徽章和启动/停止按钮。
 *
 * 面板自己持有一份运行状态，不依赖页面传入——每个实例一份，互不干扰。
 * 启动前先 `activate` 验证文件；activate 失败时进入「缺文件」态并显示原因。
 * `GameRunStatus` 只是命令的一次性返回值，不是事件流；面板不知道进程何时
 * 在外部退出，所以挂载时的初始标签只引用清单里的上次状态、不声称新鲜。
 */
export function InstanceLaunchPanel({
  instanceId,
  initialState = "ready",
  disabled = false,
}: InstanceLaunchPanelProps) {
  const desktop = isTauri();
  const [offlineName, setOfflineName] = useState("Player");
  // 面板只显示内存上限，不提供修改；调整入口在「设置 → 内存」
  const [memoryMiB] = useState(loadMemoryMiB);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<GameRunStatus | null>(null);
  const [activateError, setActivateError] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);

  if (!desktop) {
    return (
      <div className="launch-panel launch-panel-desktop-only">
        <Text className="muted">需要桌面版才能启动实例</Text>
      </div>
    );
  }

  const nameCheck = validatePlayerName(offlineName);
  const phase = launchPhase(status, busy, activateError);
  // 还没拿到任何命令结果时显示清单给的初始标签（「未验证」式），而非「可启动」
  const view =
    phase === "installed-ready" ? initialLabel(initialState) : describePhase(phase);
  const exitText = describeExit(status);
  const startDisabled =
    disabled || busy || !canStart(phase) || !nameCheck.ok;

  const handleStart = () => {
    if (!nameCheck.ok) {
      return;
    }
    setBusy(true);
    setActivateError(null);
    setRunError(null);
    activateInstalledInstance(instanceId)
      .then(() =>
        // activate 失败走外层 catch（缺文件）；start 失败在这里就地消化，
        // 用合成 failed 状态区分「文件没补齐」和「启动本身失败」
        startInstalledInstance(instanceId, nameCheck.name, memoryMiB).then(
          (result) => {
            setStatus(result);
          },
          (error: unknown) => {
            setStatus({
              instanceId,
              state: "failed",
              pid: 0,
              exitCode: null,
              discardedLogs: 0,
            });
            setRunError(describeCommandError(error));
          },
        ),
      )
      .catch((error: unknown) => {
        setActivateError(describeCommandError(error));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  const handleStop = () => {
    setBusy(true);
    setRunError(null);
    stopInstalledInstance(instanceId)
      .then((result) => {
        setStatus(result);
      })
      .catch((error: unknown) => {
        setRunError(describeCommandError(error));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  return (
    <div className="launch-panel">
      <div className="launch-panel-heading">
        <Text weight="semibold" className="launch-panel-title">
          {instanceId}
        </Text>
        <Badge appearance="tint" color={view.tone}>
          {view.label}
        </Badge>
      </div>

      <div className="launch-panel-fields">
        <label className="launch-panel-field">
          <Text className="muted">离线玩家名</Text>
          <Input
            value={offlineName}
            onChange={(_, data) => setOfflineName(data.value)}
            aria-label="离线玩家名"
            maxLength={16}
            disabled={busy || disabled}
          />
        </label>
        <Text className="muted launch-panel-memory">
          内存 {formatMemoryMiB(memoryMiB)}
        </Text>
      </div>
      {!nameCheck.ok ? (
        <Text role="alert" className="field-error">
          {nameCheck.error}
        </Text>
      ) : null}

      <div className="launch-panel-actions">
        {canStop(phase) ? (
          <Button appearance="primary" disabled={busy} onClick={handleStop}>
            停止
          </Button>
        ) : (
          <Button
            appearance="primary"
            disabled={startDisabled}
            onClick={handleStart}
          >
            {buttonLabel(phase)}
          </Button>
        )}
      </div>

      {phase === "running" && status !== null ? (
        <Text className="muted launch-panel-run">PID {status.pid}</Text>
      ) : null}
      {exitText ? (
        <Text className="muted launch-panel-run">{exitText}</Text>
      ) : null}
      {phase === "missing-files" && activateError !== null ? (
        <Text role="alert" className="field-error">
          {activateError}
        </Text>
      ) : null}
      {runError !== null ? (
        <Text role="alert" className="field-error">
          {runError}
        </Text>
      ) : null}
    </div>
  );
}
