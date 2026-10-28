import { createDarkTheme, createLightTheme } from "@fluentui/react-components";
import type { BrandVariants, Theme } from "@fluentui/react-components";

/**
 * Aether 的品牌色阶。Fluent 2 要求 16 个从深到浅的色阶（10–160），
 * 主题里的主色、悬停、按下、前景色都从这里取。
 *
 * 这里用固定色相、固定饱和度、均匀变化亮度来生成，而不是手写 16 个十六进制：
 * 想换主色只需要改色相。色相取 Logo 渐变中间的蓝色。
 */
const brandHue = 218;
const brandSaturation = 88;
const darkest = 7;
const lightest = 94;

function hslToHex(hue: number, saturation: number, lightness: number): string {
  const s = saturation / 100;
  const l = lightness / 100;
  const k = (n: number) => (n + hue / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const channel = (n: number) => {
    const value = l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
    return Math.round(value * 255)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${channel(0)}${channel(8)}${channel(4)}`;
}

function buildRamp(): BrandVariants {
  const ramp: Record<number, string> = {};
  for (let index = 0; index < 16; index += 1) {
    const lightness = darkest + (index * (lightest - darkest)) / 15;
    // 两端降低饱和度，避免最深和最浅的色阶显得刺眼
    const edge = Math.min(index, 15 - index);
    const saturation = brandSaturation - Math.max(0, 3 - edge) * 8;
    ramp[(index + 1) * 10] = hslToHex(brandHue, saturation, lightness);
  }
  return ramp as unknown as BrandVariants;
}

export const brandRamp: BrandVariants = buildRamp();

/** 系统界面字体。Windows 11 的 Segoe UI Variable 在前，其余平台回落到系统字体。 */
const fontFamilyBase =
  '"Segoe UI Variable Text", "Segoe UI", "Microsoft YaHei UI", system-ui, -apple-system, sans-serif';

export const aetherLightTheme: Theme = {
  ...createLightTheme(brandRamp),
  fontFamilyBase,
};

export const aetherDarkTheme: Theme = {
  ...createDarkTheme(brandRamp),
  fontFamilyBase,
  // Fluent 在深色主题里用更浅的色阶做品牌前景，否则文字对比度不够
  colorBrandForeground1: brandRamp[110],
  colorBrandForeground2: brandRamp[120],
};
