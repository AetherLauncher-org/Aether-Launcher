import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * 投影原理图的 IPC：打开（给 3D 预览用）、转换、转换对话框要的选项。
 *
 * 命令定义在 `src-tauri/src/commands/schematics.rs`。前端只传实例 ID、纯文件名、格式标签
 * 和数据版本号，从不传路径。文件内容不可信，后端已经限了大小，这里的数据只用来画和显示。
 */

export type SchematicFormat = "litematic" | "schem" | "nbt";

export type PaletteEntry = {
  /** `minecraft:oak_stairs` */
  name: string;
  properties: { key: string; value: string }[];
  air: boolean;
  /** 用到这个状态的方块数 */
  count: number;
};

export type ItemView = {
  slot: number | null;
  id: string;
  count: number;
  /** 物品的附加数据（`components` 或旧的 `tag`）的文本 */
  data: string | null;
};

export type BlockEntityView = {
  x: number;
  y: number;
  z: number;
  id: string;
  snbt: string;
  items: ItemView[];
};

export type SchematicView = {
  fileName: string;
  format: SchematicFormat;
  name: string;
  author: string;
  description: string;
  dataVersion: number | null;
  size: [number, number, number];
  solidBlocks: number;
  palette: PaletteEntry[];
  /** base64，一对对小端 u32：调色板序号、连续个数 */
  runs: string;
  runCount: number;
  blockEntities: BlockEntityView[];
  /** 方块实体总数；比 `blockEntities` 多时说明列表被截断了 */
  blockEntityTotal: number;
};

export type ConversionResult = {
  fileName: string;
  format: SchematicFormat;
  dataVersion: number | null;
  renamedBlocks: number;
  warnings: string[];
};

export type ConversionTargets = {
  formats: { label: SchematicFormat; name: string }[];
  versions: { dataVersion: number; name: string }[];
};

declare module "./call" {
  interface CommandArgs {
    read_instance_schematic: { id: string; name: string };
    convert_instance_schematic: {
      id: string;
      name: string;
      format: SchematicFormat;
      dataVersion: number | null;
    };
    schematic_conversion_targets: void;
  }

  interface CommandResults {
    read_instance_schematic: SchematicView;
    convert_instance_schematic: ConversionResult;
    schematic_conversion_targets: ConversionTargets;
  }
}

function requireDesktop(): void {
  if (!isTauri()) {
    throw new Error("查看投影需要桌面版。");
  }
}

export async function readInstanceSchematic(id: string, name: string): Promise<SchematicView> {
  requireDesktop();
  return callCommand("read_instance_schematic", { id, name });
}

/** 转换成另一种格式（和可选的另一个 Minecraft 数据版本），存成同一文件夹里的新文件。 */
export async function convertInstanceSchematic(
  id: string,
  name: string,
  format: SchematicFormat,
  dataVersion: number | null,
): Promise<ConversionResult> {
  requireDesktop();
  return callCommand("convert_instance_schematic", { id, name, format, dataVersion });
}

export async function getConversionTargets(): Promise<ConversionTargets> {
  requireDesktop();
  return callCommand("schematic_conversion_targets");
}
