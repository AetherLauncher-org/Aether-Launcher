import { Button, Text } from "@fluentui/react-components";
import {
  ArrowDownload20Regular,
  Box24Regular,
  Person24Regular,
  Premium24Regular,
} from "@fluentui/react-icons";
import { useMemo } from "react";
import type { CSSProperties, ReactNode } from "react";
import { GameControls } from "../components/GameControls";
import { InstallFailure, InstallProgressCard } from "../components/InstallProgressCard";
import { InstanceCover } from "../components/InstanceCover";
import { PageHeader } from "../components/PageHeader";
import { DesktopOnlyNotice } from "../components/PageState";
import { discoverJavaRuntimes } from "../ipc";
import { sortInstances } from "../lib/instances";
import { useQuery } from "../lib/query";
import { loadLastPlayedId } from "../lib/settings";
import { useAccounts } from "../state/accounts";
import { useInstall } from "../state/install";
import { useInstances } from "../state/instances";
import { useNavigation } from "../state/navigation";

type HomePageProps = {
  greeting: string;
  summary: string;
};

type StatProps = {
  icon: ReactNode;
  label: string;
  value: string;
  hint: string;
  onClick: () => void;
  index: number;
};

function Stat({ icon, label, value, hint, onClick, index }: StatProps) {
  return (
    <button type="button" className="stat-card" onClick={onClick} style={{ "--i": index } as CSSProperties}>
      <span className="stat-icon" aria-hidden="true">
        {icon}
      </span>
      <span className="stat-copy">
        <Text size={200} className="muted">
          {label}
        </Text>
        <Text size={500} weight="semibold" truncate wrap={false}>
          {value}
        </Text>
        <Text size={200} className="muted">
          {hint}
        </Text>
      </span>
    </button>
  );
}

/** 主页：继续游戏、当前状态和正在进行的安装。 */
export function HomePage({ greeting, summary }: HomePageProps) {
  const { desktop, instances } = useInstances();
  const { active } = useAccounts();
  const { job, cancel, start } = useInstall();
  const { navigate } = useNavigation();
  const { data: runtimes } = useQuery(discoverJavaRuntimes, desktop);

  const featured = useMemo(() => {
    const lastId = loadLastPlayedId();
    return (
      instances.find((instance) => instance.id === lastId) ??
      sortInstances(instances).find((instance) => instance.state === "ready") ??
      instances[0] ??
      null
    );
  }, [instances]);

  const javaCount = runtimes?.length ?? 0;
  const javaValue = runtimes === null ? "检测中…" : javaCount === 0 ? "未检测到" : `${javaCount} 个`;
  const javaHint =
    runtimes === null || javaCount === 0
      ? "需要 Java 才能启动游戏"
      : `最高 Java ${Math.max(...runtimes.map((runtime) => runtime.majorVersion))}`;

  return (
    <>
      <PageHeader title={greeting} description={summary} />

      {!desktop ? <DesktopOnlyNotice body="浏览器预览里没有本机实例。用桌面版可以安装、管理和启动游戏。" /> : null}

      <section className="hero-card" aria-label="继续游戏">
        {featured !== null ? (
          <>
            <InstanceCover version={featured.version} className="hero-cover" />
            <div className="hero-copy">
              <Text size={200} className="muted">
                继续游戏
              </Text>
              <Text as="h2" size={700} weight="semibold" className="hero-title" truncate wrap={false}>
                {featured.name}
              </Text>
              <Text className="muted">Vanilla {featured.version}</Text>
            </div>
            <div className="hero-action">
              <GameControls instance={featured} size="large" />
            </div>
          </>
        ) : (
          <>
            <div className="hero-cover hero-empty" aria-hidden="true">
              <ArrowDownload20Regular />
            </div>
            <div className="hero-copy">
              <Text as="h2" size={700} weight="semibold" className="hero-title">
                开始你的第一个实例
              </Text>
              <Text className="muted">
                选一个游戏版本，Aether 会从 Mojang 官方下载并逐个校验文件，完成后即可启动。
              </Text>
            </div>
            <div className="hero-action">
              <Button
                appearance="primary"
                size="large"
                icon={<ArrowDownload20Regular />}
                disabled={!desktop}
                onClick={() => navigate("download")}
              >
                去下载
              </Button>
            </div>
          </>
        )}
      </section>

      {job !== null && job.status === "running" ? <InstallProgressCard job={job} onCancel={cancel} /> : null}
      {job !== null && (job.status === "error" || job.status === "cancelled") ? (
        <InstallFailure job={job} onRetry={() => void start(job.version, job.name)} />
      ) : null}

      <section className="stat-row" aria-label="当前状态">
        <Stat
          index={0}
          icon={<Box24Regular />}
          label="实例"
          value={`${instances.length} 个`}
          hint={instances.length === 0 ? "还没有安装任何版本" : "点击管理"}
          onClick={() => navigate("instances")}
        />
        <Stat
          index={1}
          icon={<Person24Regular />}
          label="当前账户"
          value={active?.name ?? "未选择"}
          hint={active ? "离线账户" : "添加账户后才能启动"}
          onClick={() => navigate("accounts")}
        />
        <Stat
          index={2}
          icon={<Premium24Regular />}
          label="Java 运行时"
          value={javaValue}
          hint={javaHint}
          onClick={() => navigate("settings")}
        />
      </section>
    </>
  );
}
