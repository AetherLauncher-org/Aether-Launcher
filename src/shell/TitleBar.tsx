import { ProgressBar, Text } from "@fluentui/react-components";
import { ArrowDownload16Regular } from "@fluentui/react-icons";
import { useEffect, useState } from "react";
import {
  closeWindow,
  isTauri,
  minimizeWindow,
  toggleMaximizeWindow,
  watchMaximized,
} from "../ipc";
import { overallFraction } from "../lib/install";
import { useInstall } from "../state/install";
import { useNavigation } from "../state/navigation";
import { Logo } from "./Logo";

/**
 * 自绘标题栏。窗口没有系统标题栏，所以这里负责拖动区域（`data-tauri-drag-region`）
 * 和最小化、最大化、关闭三个按钮。安装进行时，右侧会出现一个进度条，点击回到下载页。
 */
export function TitleBar() {
  const desktop = isTauri();
  const [maximized, setMaximized] = useState(false);
  const { job } = useInstall();
  const { navigate } = useNavigation();

  useEffect(() => {
    let cancelled = false;
    let stop: (() => void) | undefined;
    void watchMaximized(setMaximized).then((unlisten) => {
      if (cancelled) {
        unlisten();
      } else {
        stop = unlisten;
      }
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  const installing = job !== null && job.status === "running";
  const percent = installing ? Math.round(overallFraction(job.progress) * 100) : 0;

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand" data-tauri-drag-region>
        <Logo size={18} />
        <Text size={200} className="titlebar-title" data-tauri-drag-region>
          Aether Launcher
        </Text>
      </div>

      <div className="titlebar-spacer" data-tauri-drag-region />

      {installing ? (
        <button
          type="button"
          className="titlebar-install"
          onClick={() => navigate("download")}
          aria-label={`正在安装 ${job.version}，${percent}%，点击查看`}
        >
          <ArrowDownload16Regular aria-hidden="true" />
          <span className="titlebar-install-text">
            安装 {job.version} · {percent}%
          </span>
          <ProgressBar
            className="titlebar-install-bar"
            thickness="medium"
            value={overallFraction(job.progress)}
          />
        </button>
      ) : null}

      {desktop ? (
        <div className="caption-buttons">
          <button
            type="button"
            className="caption-button"
            aria-label="最小化"
            onClick={() => void minimizeWindow()}
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <path d="M0 5h10" stroke="currentColor" strokeWidth="1" fill="none" />
            </svg>
          </button>
          <button
            type="button"
            className="caption-button"
            aria-label={maximized ? "还原" : "最大化"}
            onClick={() => void toggleMaximizeWindow()}
          >
            {maximized ? (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <path
                  d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z"
                  stroke="currentColor"
                  strokeWidth="1"
                  fill="none"
                />
              </svg>
            ) : (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <rect x=".5" y=".5" width="9" height="9" stroke="currentColor" fill="none" />
              </svg>
            )}
          </button>
          <button
            type="button"
            className="caption-button caption-close"
            aria-label="关闭"
            onClick={() => void closeWindow()}
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" strokeWidth="1" fill="none" />
            </svg>
          </button>
        </div>
      ) : null}
    </header>
  );
}
