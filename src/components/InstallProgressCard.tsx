import { Button, MessageBar, MessageBarBody, ProgressBar, Text } from "@fluentui/react-components";
import { Checkmark16Filled } from "@fluentui/react-icons";
import {
  describeStage,
  formatBytes,
  formatRemaining,
  formatSpeed,
  installStages,
  overallFraction,
  stageFraction,
} from "../lib/install";
import type { InstallJob } from "../state/install";

type InstallProgressCardProps = {
  job: InstallJob;
  onCancel: () => void;
};

/** 安装进行中的卡片：四步指示、当前阶段进度、字节数、速度和取消按钮。 */
export function InstallProgressCard({ job, onCancel }: InstallProgressCardProps) {
  const progress = job.progress;
  const currentStage = progress?.stage ?? "metadata";
  const currentIndex = installStages.indexOf(currentStage);
  const percent = Math.round(overallFraction(progress) * 100);
  const bytesLeft = progress ? Math.max(0, progress.bytesTotal - progress.bytesDone) : 0;
  const remaining = formatRemaining(bytesLeft, job.speed);

  return (
    <section className="status-panel install-card" aria-label="安装进度" aria-live="polite">
      <div className="install-card-head">
        <Text as="h2" size={400} weight="semibold">
          正在安装 {job.version}
        </Text>
        <Text className="install-percent" weight="semibold" size={500}>
          {percent}%
        </Text>
      </div>

      <ol className="install-steps">
        {installStages.map((stage, index) => {
          const state = index < currentIndex ? "done" : index === currentIndex ? "current" : "pending";
          return (
            <li key={stage} className={`install-step ${state}`} aria-current={state === "current" ? "step" : undefined}>
              <span className="install-step-dot" aria-hidden="true">
                {state === "done" ? <Checkmark16Filled /> : index + 1}
              </span>
              <span className="install-step-label">{describeStage(stage)}</span>
            </li>
          );
        })}
      </ol>

      <ProgressBar
        className="install-bar"
        thickness="large"
        value={progress && progress.bytesTotal + progress.filesTotal > 0 ? stageFraction(progress) : undefined}
        aria-label={`${describeStage(currentStage)}进度`}
      />

      <div className="install-stats">
        {progress && progress.bytesTotal > 0 ? (
          <Text className="muted" size={200}>
            {formatBytes(progress.bytesDone)} / {formatBytes(progress.bytesTotal)}
          </Text>
        ) : (
          <Text className="muted" size={200}>
            {describeStage(currentStage)}…
          </Text>
        )}
        {progress && progress.filesTotal > 1 ? (
          <Text className="muted" size={200}>
            文件 {progress.filesDone} / {progress.filesTotal}
          </Text>
        ) : null}
        <Text className="muted" size={200}>
          {formatSpeed(job.speed)}
          {remaining ? ` · ${remaining}` : ""}
        </Text>
      </div>

      <div className="install-actions">
        <Button appearance="secondary" onClick={onCancel}>
          取消
        </Button>
        <Text className="muted" size={200}>
          取消后已下载的文件会保留，下次继续。
        </Text>
      </div>
    </section>
  );
}

type InstallResultProps = {
  job: InstallJob;
  onRetry: () => void;
};

/** 安装失败或被取消后的提示。成功的提示由页面自己排版，因为那里要放「启动」按钮。 */
export function InstallFailure({ job, onRetry }: InstallResultProps) {
  const cancelled = job.status === "cancelled";
  return (
    <MessageBar intent={cancelled ? "warning" : "error"} className="install-failure" role="alert">
      <MessageBarBody>
        {cancelled
          ? `已取消 ${job.version} 的安装，已下载的文件会保留。`
          : `安装 ${job.version} 失败：${job.error ?? "未知错误"}`}
        <Button size="small" appearance="transparent" onClick={onRetry} className="install-retry">
          重试
        </Button>
      </MessageBarBody>
    </MessageBar>
  );
}
