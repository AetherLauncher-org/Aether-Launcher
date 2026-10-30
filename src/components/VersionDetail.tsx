import { Badge, Button, Field, Input, MessageBar, MessageBarBody, Text } from "@fluentui/react-components";
import { ArrowDownload20Regular, ArrowLeft20Regular, CheckmarkCircle24Filled } from "@fluentui/react-icons";
import { useState } from "react";
import type { FormEvent } from "react";
import type { MinecraftCatalogVersion } from "../ipc";
import { validateInstanceName } from "../lib/instances";
import {
  availability,
  describeKind,
  formatReleaseDateTime,
  kindOf,
  metadataHost,
  suggestInstanceName,
} from "../lib/versions";
import { useInstall } from "../state/install";
import { useInstances } from "../state/instances";
import { useNavigation } from "../state/navigation";
import { GameControls } from "./GameControls";
import { InstallFailure, InstallProgressCard } from "./InstallProgressCard";
import { InstanceCover } from "./InstanceCover";
import { ModLoaderPicker } from "./ModLoaderPicker";

type VersionDetailProps = {
  version: MinecraftCatalogVersion;
  isLatest: boolean;
  onBack: () => void;
};

/**
 * 单个版本的详情页：元数据在右侧，安装在左侧。安装只有一个按钮——
 * 取名字、点「安装」，剩下的（版本清单、游戏文件、资源、创建实例）由后端按顺序完成。
 */
export function VersionDetail({ version, isLatest, onBack }: VersionDetailProps) {
  const { job, start, cancel, dismiss } = useInstall();
  const { instances } = useInstances();
  const { navigate } = useNavigation();

  const existingNames = instances.map((instance) => instance.name);
  const [name, setName] = useState(() => suggestInstanceName(version.id, existingNames));
  const [touched, setTouched] = useState(false);

  const support = availability(version);
  const thisJob = job !== null && job.version === version.id ? job : null;
  const otherRunning = job !== null && job.status === "running" && job.version !== version.id;

  const duplicate = existingNames.some((existing) => existing.toLowerCase() === name.trim().toLowerCase());
  const nameError = validateInstanceName(name) ?? (duplicate ? "已经有同名实例了" : null);
  const showError = touched && nameError !== null;

  const canInstall = support.ok && !otherRunning && nameError === null;

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    setTouched(true);
    if (canInstall) {
      void start(version.id, name.trim());
    }
  };

  const handleAnother = () => {
    if (thisJob !== null) {
      setName(suggestInstanceName(version.id, [...existingNames, thisJob.name]));
      setTouched(false);
    }
    dismiss();
  };

  return (
    <div className="detail">
      <button type="button" className="back-link" onClick={onBack}>
        <ArrowLeft20Regular aria-hidden="true" />
        返回版本列表
      </button>

      <header className="detail-hero">
        <InstanceCover version={version.id} className="detail-cover" />
        <div className="detail-hero-copy">
          <div className="detail-title-row">
            <Text as="h1" size={800} weight="semibold" className="page-title">
              {version.id}
            </Text>
            {isLatest ? (
              <Badge appearance="tint" color="brand">
                最新
              </Badge>
            ) : null}
          </div>
          <Text className="page-subtitle">
            {describeKind(kindOf(version))} · 发布于 {formatReleaseDateTime(version.releaseTime)}
          </Text>
        </div>
      </header>

      <div className="detail-grid">
        <div className="detail-main">
          {!support.ok ? (
            <MessageBar intent="warning" className="page-notice">
              <MessageBarBody>{support.reason}。可以选择 1.13 或更新的版本。</MessageBarBody>
            </MessageBar>
          ) : null}

          {otherRunning && job !== null ? (
            <MessageBar intent="info" className="page-notice">
              <MessageBarBody>正在安装 {job.version}，完成后才能安装其他版本。</MessageBarBody>
            </MessageBar>
          ) : null}

          {thisJob !== null && thisJob.status === "running" ? (
            <InstallProgressCard job={thisJob} onCancel={cancel} />
          ) : thisJob !== null && thisJob.status === "done" && thisJob.instance !== null ? (
            <section className="status-panel success-card" aria-live="polite">
              <CheckmarkCircle24Filled className="success-icon" aria-hidden="true" />
              <Text as="h2" size={500} weight="semibold">
                安装完成
              </Text>
              <Text className="muted">实例「{thisJob.instance.name}」已创建，所有文件都已通过校验，可以启动了。</Text>
              <div className="success-actions">
                <GameControls instance={thisJob.instance} size="large" hideBadge />
                <Button onClick={() => navigate("instances")}>查看实例</Button>
                <Button appearance="subtle" onClick={handleAnother}>
                  再装一个
                </Button>
              </div>
            </section>
          ) : (
            <>
              {thisJob !== null ? <InstallFailure job={thisJob} onRetry={() => void start(thisJob.version, thisJob.name)} /> : null}
              <form className="status-panel install-form" onSubmit={handleSubmit}>
                <Text as="h2" size={400} weight="semibold">
                  安装这个版本
                </Text>
                <Field
                  label="实例名称"
                  validationMessage={showError ? (nameError ?? undefined) : undefined}
                  validationState={showError ? "error" : "none"}
                  hint="安装完成后会用这个名字创建实例，之后可以随时区分。"
                >
                  <Input
                    value={name}
                    onChange={(_, data) => {
                      setName(data.value);
                      setTouched(true);
                    }}
                    disabled={!support.ok}
                  />
                </Field>
                <div className="install-form-actions">
                  <Button
                    type="submit"
                    appearance="primary"
                    size="large"
                    icon={<ArrowDownload20Regular />}
                    disabled={!support.ok || otherRunning}
                  >
                    安装
                  </Button>
                  <Text className="muted" size={200}>
                    文件校验值全部来自 Mojang 官方清单，下载后会逐个核对。
                  </Text>
                </div>
              </form>
              <ModLoaderPicker gameVersion={version.id} />
            </>
          )}
        </div>

        <aside className="detail-side" aria-label="版本信息">
          <section className="status-panel">
            <Text as="h2" size={400} weight="semibold">
              版本信息
            </Text>
            <dl className="meta-list">
              <div>
                <dt>版本号</dt>
                <dd>{version.id}</dd>
              </div>
              <div>
                <dt>类型</dt>
                <dd>{describeKind(kindOf(version))}</dd>
              </div>
              <div>
                <dt>发布时间</dt>
                <dd>{formatReleaseDateTime(version.releaseTime)}</dd>
              </div>
              <div>
                <dt>清单来源</dt>
                <dd>{metadataHost(version.url)}</dd>
              </div>
              <div>
                <dt>清单 SHA-1</dt>
                <dd className="mono" title={version.sha1}>
                  {version.sha1.slice(0, 12)}…
                </dd>
              </div>
              <div>
                <dt>支持情况</dt>
                <dd>{support.ok ? "支持安装与启动" : "暂不支持"}</dd>
              </div>
            </dl>
          </section>
        </aside>
      </div>
    </div>
  );
}
