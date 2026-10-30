/**
 * 方块的显示方式：颜色、画不画、透不透明、能不能被指到。
 *
 * 启动器不带也不读取 Mojang 的纹理，所以每个方块是一块纯色，颜色来自一张常见方块的表，
 * 表里没有的按名字里的关键词（木头、石头、染色方块……）找最接近的，再没有就由名字算出
 * 一个固定的颜色，同一个方块永远是同一个颜色。
 */

export type BlockLook = {
  rgb: readonly [number, number, number];
  /** 要不要画出来（空气不画） */
  draw: boolean;
  /** 不透明：会挡住相邻方块的面 */
  occlude: boolean;
  /** 半透明（玻璃、冰、水）：画出来，但不挡住后面的东西 */
  translucent: boolean;
  /** 鼠标能不能指到它 */
  pick: boolean;
};

const hex = (value: number): [number, number, number] => [(value >> 16) & 255, (value >> 8) & 255, value & 255];

const DYES: Record<string, number> = {
  white: 0xf9fffe,
  orange: 0xf9801d,
  magenta: 0xc74ebd,
  light_blue: 0x3ab3da,
  yellow: 0xfed83d,
  lime: 0x80c71f,
  pink: 0xf38baa,
  gray: 0x474f52,
  light_gray: 0x9d9d97,
  cyan: 0x169c9c,
  purple: 0x8932b8,
  blue: 0x3c44aa,
  brown: 0x835432,
  green: 0x5e7c16,
  red: 0xb02e26,
  black: 0x1d1d21,
};

const WOODS: Record<string, number> = {
  oak: 0xb8945f,
  spruce: 0x7b5a35,
  birch: 0xd5c98c,
  jungle: 0xb88764,
  acacia: 0xba683b,
  dark_oak: 0x4a3018,
  mangrove: 0x76363a,
  cherry: 0xe0b7ba,
  bamboo: 0xc3b24a,
  crimson: 0x7a3a5b,
  warped: 0x2b8478,
  pale_oak: 0xe3d7cc,
};

const EXACT: Record<string, number> = {
  stone: 0x7d7d7d,
  cobblestone: 0x7f7f7f,
  mossy_cobblestone: 0x6a7f5b,
  stone_bricks: 0x7a7a7a,
  mossy_stone_bricks: 0x6f7f66,
  cracked_stone_bricks: 0x757575,
  chiseled_stone_bricks: 0x787878,
  smooth_stone: 0x9e9e9e,
  andesite: 0x888888,
  polished_andesite: 0x8c8c8c,
  diorite: 0xbcbcbc,
  polished_diorite: 0xc2c2c2,
  granite: 0x9a6c58,
  polished_granite: 0xa1705b,
  deepslate: 0x4c4c50,
  cobbled_deepslate: 0x4d4d51,
  deepslate_bricks: 0x464649,
  deepslate_tiles: 0x383a3d,
  tuff: 0x6c6d63,
  calcite: 0xe0e0da,
  basalt: 0x4b4b50,
  blackstone: 0x2a2429,
  polished_blackstone: 0x353036,
  polished_blackstone_bricks: 0x302b31,
  netherrack: 0x6f3635,
  nether_bricks: 0x2c161a,
  red_nether_bricks: 0x450709,
  soul_sand: 0x514034,
  glowstone: 0xffbc5e,
  magma_block: 0x8d3c14,
  obsidian: 0x15121f,
  crying_obsidian: 0x2c0a60,
  end_stone: 0xdfe0a8,
  end_stone_bricks: 0xdbe2a3,
  purpur_block: 0xa97ba9,
  purpur_pillar: 0xab7eab,
  dirt: 0x866043,
  coarse_dirt: 0x77553a,
  rooted_dirt: 0x90694b,
  grass_block: 0x6a9a43,
  podzol: 0x5a3f1d,
  mycelium: 0x6f6369,
  dirt_path: 0x947a43,
  farmland: 0x6d4a2c,
  mud: 0x3c3837,
  mud_bricks: 0x89684f,
  clay: 0xa0a6b4,
  sand: 0xdbcfa3,
  red_sand: 0xbe6621,
  sandstone: 0xdbd3a0,
  smooth_sandstone: 0xdfd7a6,
  cut_sandstone: 0xdcd4a3,
  red_sandstone: 0xb5602b,
  gravel: 0x837f7d,
  snow_block: 0xf8fefe,
  snow: 0xf8fefe,
  powder_snow: 0xf4fbfb,
  ice: 0x91b8fe,
  packed_ice: 0x8cb3f0,
  blue_ice: 0x74a8fd,
  water: 0x3f76e4,
  lava: 0xd96514,
  leaves: 0x3f7a2a,
  oak_leaves: 0x3f7a2a,
  spruce_leaves: 0x2e5a3a,
  birch_leaves: 0x6a8f3f,
  jungle_leaves: 0x2f8a1d,
  acacia_leaves: 0x4a8a2a,
  dark_oak_leaves: 0x2f6a1f,
  azalea_leaves: 0x5a8a30,
  cherry_leaves: 0xe6a6c0,
  mangrove_leaves: 0x4a7a2a,
  moss_block: 0x596d2d,
  hay_block: 0xb59a16,
  bookshelf: 0x9c7f4f,
  crafting_table: 0x9a6a3a,
  furnace: 0x7d7d7d,
  blast_furnace: 0x6c6c70,
  smoker: 0x6e6a60,
  chest: 0x9b6d1a,
  trapped_chest: 0x9b6d1a,
  ender_chest: 0x1f3a3a,
  barrel: 0x86633a,
  tnt: 0xdb2f1f,
  bricks: 0x96534a,
  iron_block: 0xd8d8d8,
  gold_block: 0xf9d849,
  diamond_block: 0x62ede4,
  emerald_block: 0x2cb857,
  lapis_block: 0x1d4a9a,
  redstone_block: 0xaa1c0b,
  coal_block: 0x101010,
  netherite_block: 0x4a4549,
  copper_block: 0xc06e4d,
  quartz_block: 0xece6df,
  smooth_quartz: 0xece6df,
  quartz_bricks: 0xe8e2da,
  prismarine: 0x63a897,
  prismarine_bricks: 0x63ab9a,
  dark_prismarine: 0x345c4c,
  sea_lantern: 0xd8ece4,
  shroomlight: 0xf09a4a,
  slime_block: 0x70c05a,
  honey_block: 0xf9b72a,
  sponge: 0xc9c34a,
  wet_sponge: 0xa2a032,
  glass: 0xc0f0f4,
  tinted_glass: 0x2d2b33,
  iron_bars: 0x8a8a8a,
  torch: 0xffd24a,
  lantern: 0xe0a64a,
  ladder: 0x8a6a3a,
  cobweb: 0xe8e8e8,
  bedrock: 0x555555,
  spawner: 0x1a2a3a,
};

const FAMILIES: ReadonlyArray<readonly [RegExp, number]> = [
  [/copper/, 0xc06e4d],
  [/quartz/, 0xece6df],
  [/prismarine/, 0x63a897],
  [/purpur/, 0xa97ba9],
  [/end_stone/, 0xdfe0a8],
  [/nether_brick/, 0x2c161a],
  [/blackstone/, 0x2a2429],
  [/deepslate/, 0x4c4c50],
  [/mud_brick/, 0x89684f],
  [/red_sandstone/, 0xb5602b],
  [/sandstone/, 0xdbd3a0],
  [/stone_brick/, 0x7a7a7a],
  [/cobblestone/, 0x7f7f7f],
  [/brick/, 0x96534a],
  [/granite/, 0x9a6c58],
  [/diorite/, 0xbcbcbc],
  [/andesite/, 0x888888],
  [/tuff/, 0x6c6d63],
  [/stone/, 0x7d7d7d],
  [/leaves/, 0x3f7a2a],
  [/(sapling|grass|fern|flower|tulip|rose|dandelion|poppy)/, 0x5a9a3a],
  [/ore$/, 0x7d7d7d],
];

/** 名字 → 固定的颜色：同一个名字永远得到同一个颜色。 */
function hashColor(name: string): [number, number, number] {
  let h = 2166136261;
  for (const c of name) {
    h = Math.imul(h ^ c.codePointAt(0)!, 16777619) >>> 0;
  }
  const hue = (h % 360) / 360;
  const s = 0.38;
  const v = 0.78;
  const i = Math.floor(hue * 6);
  const f = hue * 6 - i;
  const p = v * (1 - s);
  const q = v * (1 - f * s);
  const t = v * (1 - (1 - f) * s);
  const [r, g, b] = [
    [v, t, p],
    [q, v, p],
    [p, v, t],
    [p, q, v],
    [t, p, v],
    [v, p, q],
  ][i % 6];
  return [Math.round(r * 255), Math.round(g * 255), Math.round(b * 255)];
}

function shade([r, g, b]: [number, number, number], factor: number): [number, number, number] {
  return [Math.round(r * factor), Math.round(g * factor), Math.round(b * factor)];
}

function colourFor(id: string): [number, number, number] {
  const exact = EXACT[id];
  if (exact !== undefined) {
    return hex(exact);
  }
  // 染色方块：`red_concrete`、`light_blue_wool`…
  for (const [dye, value] of Object.entries(DYES)) {
    if (id.startsWith(`${dye}_`)) {
      const rest = id.slice(dye.length + 1);
      if (/^(terracotta|glazed_terracotta)/.test(rest)) {
        return shade(hex(value), 0.62);
      }
      if (/^(concrete|concrete_powder|wool|carpet|stained_glass|stained_glass_pane|shulker_box|bed|banner|candle|candle_cake)/.test(rest)) {
        return hex(value);
      }
    }
  }
  if (id === "terracotta") {
    return hex(0x985e43);
  }
  // 木头类：前缀决定木种；`stripped_` 稍亮。
  const stripped = id.startsWith("stripped_");
  const base = stripped ? id.slice("stripped_".length) : id;
  for (const [wood, value] of Object.entries(WOODS).sort((a, b) => b[0].length - a[0].length)) {
    if (base.startsWith(`${wood}_`)) {
      return stripped ? shade(hex(value), 1.12) : hex(value);
    }
  }
  for (const [pattern, value] of FAMILIES) {
    if (pattern.test(id)) {
      return hex(value);
    }
  }
  return hashColor(id);
}

const INVISIBLE = new Set(["air", "cave_air", "void_air", "light", "barrier", "structure_void"]);
const NOT_PICKABLE = new Set(["air", "cave_air", "void_air"]);
const TRANSLUCENT = /(^|_)(glass|glass_pane)$|^(ice|frosted_ice|water|tinted_glass|slime_block|honey_block)$/;

/** `minecraft:oak_planks` → `oak_planks`；别的命名空间原样保留前缀。 */
export function shortId(name: string): string {
  return name.startsWith("minecraft:") ? name.slice("minecraft:".length) : name;
}

export function blockLook(name: string): BlockLook {
  const id = shortId(name);
  const invisible = INVISIBLE.has(id);
  const translucent = !invisible && (TRANSLUCENT.test(id) || id === "tinted_glass");
  return {
    rgb: colourFor(id),
    draw: !invisible,
    occlude: !invisible && !translucent,
    translucent,
    pick: !NOT_PICKABLE.has(id),
  };
}

/** `minecraft:oak_planks` → `Oak Planks`。给人看的名字（启动器没有语言文件，按 id 推）。 */
export function displayBlockName(name: string): string {
  const [namespace, path] = name.includes(":") ? (name.split(/:(.*)/s) as [string, string]) : ["minecraft", name];
  const text = path
    .split("_")
    .filter((word) => word.length > 0)
    .map((word) => word[0].toUpperCase() + word.slice(1))
    .join(" ");
  return namespace === "minecraft" ? text : `${text} (${namespace})`;
}

/** `#rrggbb`，给小色块用。 */
export function cssColor(rgb: readonly [number, number, number]): string {
  return `#${rgb.map((value) => value.toString(16).padStart(2, "0")).join("")}`;
}
