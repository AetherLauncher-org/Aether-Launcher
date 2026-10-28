import { isTauri } from "../ipc";

type HighEntropyNavigator = Navigator & {
  userAgentData?: {
    getHighEntropyValues?: (hints: string[]) => Promise<{ platformVersion?: string }>;
  };
};

/** Windows 11 在 `platformVersion` 里报告主版本 13 及以上（Windows 10 是 1–10）。 */
export function isWindows11OrLater(platformVersion: string | undefined): boolean {
  if (platformVersion === undefined) {
    return false;
  }
  const major = Number.parseInt(platformVersion.split(".")[0] ?? "", 10);
  return Number.isFinite(major) && major >= 13;
}

/**
 * 桌面版窗口是否使用了 Mica 材质。
 *
 * 窗口在后端以 Mica 创建；材质只在 Windows 11 上生效。其他系统上窗口背景是透明的，
 * 前端必须自己铺实色，否则会透出桌面。浏览器预览里没有窗口材质，一律返回 false。
 */
export async function detectMica(): Promise<boolean> {
  if (!isTauri()) {
    return false;
  }
  const getValues = (navigator as HighEntropyNavigator).userAgentData?.getHighEntropyValues;
  if (getValues === undefined) {
    return false;
  }
  try {
    const values = await (navigator as HighEntropyNavigator).userAgentData!.getHighEntropyValues!([
      "platformVersion",
    ]);
    return isWindows11OrLater(values.platformVersion);
  } catch {
    return false;
  }
}
