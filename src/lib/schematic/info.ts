import type { BlockEntityView, ItemView, SchematicView } from "../../ipc/schematics";
import { blockLook, cssColor, displayBlockName } from "./colors";
import type { Hit } from "./picker";

/**
 * 指到一个方块时显示的信息（Jade 风格）：名字和 ID、方块状态、坐标，方块实体的话还有它的
 * 数据和容器里的物品。只做数据整理，画在 `SchematicViewer` 里。
 */

export type HoverInfo = {
  index: number;
  title: string;
  /** `minecraft:oak_stairs` */
  id: string;
  swatch: string;
  /** `facing=north` 这样的方块状态，已按名字排序 */
  properties: { key: string; value: string }[];
  /** 在投影里的坐标 */
  position: [number, number, number];
  /** 被指的面的朝向，如 `上` */
  face: string | null;
  entity: {
    id: string;
    snbt: string;
    items: ItemView[];
  } | null;
};

const FACES: Record<string, string> = {
  "0,1,0": "上",
  "0,-1,0": "下",
  "1,0,0": "东 (+x)",
  "-1,0,0": "西 (-x)",
  "0,0,1": "南 (+z)",
  "0,0,-1": "北 (-z)",
};

/** 坐标 → 方块实体，方便鼠标一动就查到。 */
export function indexEntities(view: SchematicView): Map<number, BlockEntityView> {
  const [sx, , sz] = view.size;
  const map = new Map<number, BlockEntityView>();
  for (const entity of view.blockEntities) {
    map.set((entity.y * sz + entity.z) * sx + entity.x, entity);
  }
  return map;
}

export function describeHover(
  view: SchematicView,
  entities: Map<number, BlockEntityView>,
  hit: Hit,
): HoverInfo {
  const entry = view.palette[hit.palette];
  const entity = entities.get(hit.index) ?? null;
  return {
    index: hit.index,
    title: entry === undefined ? "?" : displayBlockName(entry.name),
    id: entry?.name ?? "?",
    swatch: cssColor(blockLook(entry?.name ?? "").rgb),
    properties: [...(entry?.properties ?? [])].sort((a, b) => a.key.localeCompare(b.key)),
    position: [hit.x, hit.y, hit.z],
    face: FACES[hit.normal.join(",")] ?? null,
    entity:
      entity === null ? null : { id: entity.id, snbt: entity.snbt, items: entity.items },
  };
}

/** `minecraft:diamond_sword` ×2 → `Diamond Sword ×2`。 */
export function describeItem(item: ItemView): string {
  return `${displayBlockName(item.id)} ×${item.count}`;
}

/** 提示框里最多显示几行 NBT；完整的在固定的检查面板里。 */
export const TOOLTIP_NBT_LINES = 8;

export function nbtPreview(snbt: string, lines = TOOLTIP_NBT_LINES): { text: string; hidden: number } {
  const all = snbt.split("\n");
  return { text: all.slice(0, lines).join("\n"), hidden: Math.max(0, all.length - lines) };
}
