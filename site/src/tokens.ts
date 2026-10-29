/**
 * The site's design values. See ../DESIGN.md. Pages and components import from here and
 * never write a raw pixel number, which `design.test.ts` enforces.
 */

/** The only spacing values that exist, in pixels. */
export const spaceScale = [0, 4, 8, 12, 16, 24, 32, 48, 64] as const;

const px = (value: (typeof spaceScale)[number]) => `${value}px`;

export const space = {
  none: px(0),
  xs: px(4),
  s: px(8),
  m: px(12),
  l: px(16),
  xl: px(24),
  xxl: px(32),
  section: px(48),
  page: px(64),
} as const;

export const widths = {
  /** The one container. */
  page: "960px",
  /** A reading column: about 70 characters. */
  prose: "720px",
} as const;

export const sizes = {
  header: "56px",
  headerLogo: "28px",
  heroLogo: "56px",
  icon: "24px",
  /** The status badge column on the home page. */
  badgeColumn: "120px",
  hairline: "1px",
  /** The line under the current page in the header. */
  indicator: "2px",
} as const;

/** Breakpoints, as ready-made media queries. */
export const screen = {
  tablet: "@media (max-width: 880px)",
  phone: "@media (max-width: 600px)",
} as const;

/** Fluent `Text` size steps allowed on the site. */
export const textSize = {
  title: 900,
  section: 600,
  item: 400,
  lead: 500,
  body: 400,
  ui: 300,
  caption: 200,
} as const;
