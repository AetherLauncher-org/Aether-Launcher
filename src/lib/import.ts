import type {
  ImportResult,
  ImportSource,
  ImportSourceKind,
} from "../ipc";

/**
 * 导入对话框的纯逻辑：来源标签、跳过原因、勾选状态与结果摘要。
 *
 * 组件只负责组装，判断都在这里，方便直接单测。
 */

const sourceKindLabels: Record<ImportSourceKind, string> = {
  pcl2: "PCL2",
  hmcl: "HMCL",
  bakaxl: "BakaXL",
  prism: "Prism / MultiMC",
  folder: "普通游戏文件夹",
};

/** 来源类型的中文标签；未知值原样显示，方便对照后端。 */
export function describeImportSourceKind(kind: string): string {
  return sourceKindLabels[kind as ImportSourceKind] ?? kind;
}

/**
 * 把后端给的导入跳过原因翻译成人话。
 *
 * 后端只保证是稳定的短分类（`missing-version-json` 之类），未知值原样显示。
 */
export function describeImportSkipReason(reason: string): string {
  switch (reason) {
    case "unsafe-version-id":
      return "版本标识不能用作目录名";
    case "missing-version-json":
      return "缺少版本 JSON 文件";
    case "missing-pack-metadata":
      return "缺少 mmc-pack.json，读不出基础版本";
    case "unsafe-source":
      return "源文件是符号链接或不是普通文件";
    case "version-json-too-large":
      return "版本 JSON 超过 8 MiB";
    case "invalid-version-json":
      return "版本 JSON 格式损坏";
    case "inheritance-too-deep":
      return "版本继承层数超过 32 层";
    case "inheritance-cycle":
      return "版本继承出现循环";
    case "duplicate-name":
      // 不是错误：这个版本已经导入过了，没有要补的东西。
      return "已经导入过，不需要再导入";
    case "invalid-name":
      return "版本号不能作为实例名称";
    case "io":
      return "读写文件失败";
    default:
      return reason;
  }
}

/** 一个来源里的全部条目 ID。 */
export function allCandidateIds(source: ImportSource): string[] {
  return source.candidates.map((candidate) => candidate.id);
}

/** 勾选或取消一个条目，返回新的集合（不改动入参）。 */
export function toggleCandidate(
  selected: ReadonlySet<string>,
  id: string,
): Set<string> {
  const next = new Set(selected);
  if (next.has(id)) {
    next.delete(id);
  } else {
    next.add(id);
  }
  return next;
}

/** 全选或全不选：已经全选时清空，否则选中全部。 */
export function toggleAllCandidates(
  selected: ReadonlySet<string>,
  ids: readonly string[],
): Set<string> {
  const allSelected = ids.length > 0 && ids.every((id) => selected.has(id));
  return allSelected ? new Set() : new Set(ids);
}

/** 导入结果的一句话摘要。 */
export function summariseImport(result: ImportResult): string {
  const imported = result.imported.length;
  const skipped = result.skipped.length;
  if (imported === 0 && skipped === 0) {
    return "没有导入任何版本";
  }
  if (skipped === 0) {
    return `已导入 ${imported} 个实例`;
  }
  if (imported === 0) {
    return `没有导入成功，${skipped} 个版本被跳过`;
  }
  return `已导入 ${imported} 个实例，${skipped} 个版本被跳过`;
}
