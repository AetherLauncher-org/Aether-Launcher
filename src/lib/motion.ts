/**
 * 界面动画偏好。`full` 为默认；`reduced` 把所有过渡压到近乎瞬间，
 * 供晕动症用户或低性能机器使用。系统级的「减少动态效果」同样生效（见 CSS）。
 */
export type MotionPreference = "full" | "reduced";

export const motionStorageKey = "aether.motion";

export function isMotionPreference(value: unknown): value is MotionPreference {
  return value === "full" || value === "reduced";
}

export function loadMotionPreference(): MotionPreference {
  try {
    const stored = window.localStorage.getItem(motionStorageKey);
    return isMotionPreference(stored) ? stored : "full";
  } catch {
    return "full";
  }
}

export function saveMotionPreference(preference: MotionPreference): void {
  try {
    window.localStorage.setItem(motionStorageKey, preference);
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}
