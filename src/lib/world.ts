import type { PackState, WorldDetails } from "../ipc";

const gameModes: Record<NonNullable<WorldDetails["gameMode"]>, string> = {
  survival: "生存",
  creative: "创造",
  adventure: "冒险",
  spectator: "旁观",
};

const difficulties: Record<NonNullable<WorldDetails["difficulty"]>, string> = {
  peaceful: "和平",
  easy: "简单",
  normal: "普通",
  hard: "困难",
};

/** 模式；极限模式单独说明。`level.dat` 里没有就返回 null，界面不显示这一行。 */
export function describeMode(details: Pick<WorldDetails, "gameMode" | "hardcore">): string | null {
  if (details.gameMode === null && details.hardcore !== true) {
    return null;
  }
  const mode = details.gameMode === null ? "" : gameModes[details.gameMode];
  return details.hardcore === true ? (mode ? `${mode}（极限）` : "极限") : mode;
}

export function describeDifficulty(difficulty: WorldDetails["difficulty"]): string | null {
  return difficulty === null ? null : difficulties[difficulty];
}

export const packStateLabels: Record<PackState, { text: string; hint: string }> = {
  enabled: { text: "已启用", hint: "世界的 level.dat 里记着它是启用的" },
  disabled: { text: "已停用", hint: "世界的 level.dat 里记着它是停用的" },
  unknown: {
    text: "未知",
    hint: "level.dat 里没有它（还没进游戏加载过），或者 level.dat 读不了",
  },
};

/** 世界对话框里数据包删除前的确认文字：写明这是不可恢复的删除。 */
export function describePackRemoval(name: string, isFolder: boolean): string {
  return isFolder
    ? `文件夹「${name}」和里面的一切会被删除，无法恢复。`
    : `文件「${name}」会被删除，无法恢复。`;
}
