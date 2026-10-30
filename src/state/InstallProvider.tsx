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
import type { InstallProgress, ModLoaderKind } from "../ipc";
import { emptySpeedSample, nextSpeed } from "../lib/install";
import type { SpeedSample } from "../lib/install";
import { InstallContext } from "./install";
import type { InstallApi, InstallJob } from "./install";
import { useInstances } from "./instances";

/**
 * 安装任务放在应用层：用户可以在安装过程中切到别的页面，回来时进度还在，
 * 标题栏和主页也能显示同一份进度。后端一次只允许一个安装。
 */
export function InstallProvider({ children }: { children: ReactNode }) {
  const { reload } = useInstances();
  const [job, setJob] = useState<InstallJob | null>(null);
  const running = useRef(false);
  const sample = useRef<SpeedSample>(emptySpeedSample);

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

    void listenInstallProgress(onProgress).then((stop) => {
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const start = useCallback(
    async (
      version: string,
      name: string,
      loader: { kind: string; version: string } | null = null,
    ) => {
      if (running.current) {
        return;
      }
      running.current = true;
      sample.current = emptySpeedSample;
      setJob({
        version,
        name,
        status: "running",
        progress: null,
        speed: 0,
        instance: null,
        error: null,
      });
      try {
        // 选了加载器就走加载器那条链（后端会先装原版），否则只装原版。
        const instance =
          loader === null
            ? await startVanillaInstall(version, name)
            : (
                await installModLoaderVersion(
                  loader.kind as ModLoaderKind,
                  version,
                  loader.version,
                  name,
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

  const cancel = useCallback(() => {
    void cancelVanillaInstall();
  }, []);

  const dismiss = useCallback(() => {
    if (!running.current) {
      setJob(null);
    }
  }, []);

  const api = useMemo<InstallApi>(
    () => ({ job, start, cancel, dismiss }),
    [job, start, cancel, dismiss],
  );

  return <InstallContext.Provider value={api}>{children}</InstallContext.Provider>;
}
