import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  cancelVanillaInstall,
  describeCommandError,
  installModLoaderVersion,
  isInstallCancelled,
  listenInstallProgress,
  startVanillaInstall,
} from "../ipc";
import type { InstallProgress } from "../ipc";
import { emptySpeedSample, nextSpeed } from "../lib/install";
import type { SpeedSample } from "../lib/install";
import { InstallContext } from "./install";
import type { InstallApi, InstallJob, InstallLoader } from "./install";
import { useInstances } from "./instances";

/** 一次安装的目标。重试沿用的就是这一份，所以加载器也算在里面。 */
type InstallAttempt = {
  version: string;
  name: string;
  loader: InstallLoader | null;
};

/**
 * 安装任务放在应用层：用户可以在安装过程中切到别的页面，回来时进度还在，
 * 标题栏和主页也能显示同一份进度。后端一次只允许一个安装。
 */
export function InstallProvider({ children }: { children: ReactNode }) {
  const { reload } = useInstances();
  const [job, setJob] = useState<InstallJob | null>(null);
  const running = useRef(false);
  const sample = useRef<SpeedSample>(emptySpeedSample);
  /** 上一次安装的目标：重试用它，不用调用方手里的状态。 */
  const attempt = useRef<InstallAttempt | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const onProgress = (progress: InstallProgress) => {
      sample.current = nextSpeed(sample.current, progress, Date.now());
      const speed = sample.current.speed;
      setJob((current) =>
        current !== null && current.status === "running"
          ? { ...current, progress, speed }
          : current,
      );
    };

    // 订阅失败不该让页面崩掉：丢的只是进度条，安装本身照常跑。没有 catch 的话
    // 异常会变成 unhandled rejection。
    void listenInstallProgress(onProgress)
      .then((stop) => {
        if (cancelled) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch(() => {
        // 没有进度事件源，静默降级。
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const run = useCallback(
    async (target: InstallAttempt) => {
      if (running.current) {
        return;
      }
      running.current = true;
      sample.current = emptySpeedSample;
      setJob({
        version: target.version,
        name: target.name,
        loader: target.loader,
        status: "running",
        progress: null,
        speed: 0,
        instance: null,
        error: null,
      });
      try {
        // 选了加载器就走加载器那条链（后端会先装原版），否则只装原版。
        const instance =
          target.loader === null
            ? await startVanillaInstall(target.version, target.name)
            : (
                await installModLoaderVersion(
                  target.loader.kind,
                  target.version,
                  target.loader.version,
                  target.name,
                )
              ).instance;
        setJob((current) =>
          current === null ? current : { ...current, status: "done", instance, error: null },
        );
        reload();
      } catch (error: unknown) {
        const cancelled = isInstallCancelled(error);
        setJob((current) =>
          current === null
            ? current
            : {
                ...current,
                status: cancelled ? "cancelled" : "error",
                error: cancelled ? null : describeCommandError(error),
              },
        );
      } finally {
        running.current = false;
      }
    },
    [reload],
  );

  const start = useCallback(
    (version: string, name: string, loader: InstallLoader | null = null) => {
      // 后端一次只允许一个安装。已有安装时连「上一次的目标」也不改，免得重试装成别的。
      if (running.current) {
        return Promise.resolve();
      }
      const target: InstallAttempt = { version, name, loader };
      attempt.current = target;
      return run(target);
    },
    [run],
  );

  const retry = useCallback(() => {
    const target = attempt.current;
    if (target !== null) {
      void run(target);
    }
  }, [run]);

  const cancel = useCallback(() => {
    void cancelVanillaInstall();
  }, []);

  const dismiss = useCallback(() => {
    if (!running.current) {
      setJob(null);
    }
  }, []);

  const api = useMemo<InstallApi>(
    () => ({ job, start, retry, cancel, dismiss }),
    [job, start, retry, cancel, dismiss],
  );

  return <InstallContext.Provider value={api}>{children}</InstallContext.Provider>;
}
