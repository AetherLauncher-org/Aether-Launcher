# Site design spec

Fluent 2 is a design *language*: it gives us tokens and components, but it does not decide how a page is laid out. This file is that decision. The values are in `src/tokens.ts`, and `src/design.test.ts` fails the build if a page sneaks in a number or colour that is not listed here.

The goal is calm: one background, one accent, one rhythm. If something looks loud, remove it rather than tune it.

## Colour

- **Theme:** Fluent's stock `webLightTheme` / `webDarkTheme`, following the system. We do not make up a brand ramp for the site; Fluent's own is calibrated for contrast.
- **Surfaces:** the page and the header use `colorNeutralBackground1`. Cards have no fill and a 1px `colorNeutralStroke2` border. One surface, separated by lines, never by shadow or tint.
- **Text:** `colorNeutralForeground1` for headings and body, `colorNeutralForeground2` for leads and secondary text, `colorNeutralForeground3` for captions and dates.
- **Accent:** `colorBrand*` only on the primary button, links and the current-page underline. Nowhere else.
- **Status:** two chromatic badges at most (available = success, in progress = brand). "Planned" is a neutral outline badge.
- **Forbidden:** raw hex or `rgb()` values, gradients, glows, drop shadows, translucent overlays, blur.

## Spacing

One scale, in pixels: **4, 8, 12, 16, 24, 32, 48, 64**. Nothing else, ever.

| Use | Value |
| --- | --- |
| Gap between a heading and its content | 16 |
| Gap between paragraphs, list items | 12 and 8 |
| Gap between cards | 16 |
| Card padding | 24 (gap inside a card: 12) |
| Page gutter | 24 (16 on a phone) |
| Between sections | 48 |
| Top of the page, below the header | 64 |
| Bottom padding of the footer | 48 |

## Typography

Five steps, two weights (regular and semibold). Fluent `Text` sizes:

| Role | Size | Token |
| --- | --- | --- |
| Page title (h1) | 40 | `900` |
| Section title (h2) | 24 | `600` |
| Item title (h3) | 16 semibold | `400` |
| Lead paragraph under a title | 20 | `500` |
| Body | 16 | `400` |
| UI text (nav, cards, tables) | 14 | `300` |
| Caption (dates, footer) | 12 | `200` |

Prose is capped at 720px, which is about 70 characters, so lines stay readable.

## Layout

- One container, **960px** wide, centred, with the gutter above. Prose pages use a 720px column inside it.
- The header is 56px tall, flat, with a 1px bottom border. The logo and name on the left; links on the right.
- Cards sit on a strict grid: three columns from 880px up, two from 600px, one below. All cards in a row are the same height.
- Content is left aligned. Nothing floats on the right to fill space.

## Motion

None that loops, and none that moves layout. Fluent's own hover and focus states are enough.

## Icons

Fluent 24px regular icons, neutral colour, one per card at most.

## How it is enforced

`src/design.test.ts` reads every file under `src/` and checks:

1. every `px` value is on the scale (only `tokens.ts` may define them);
2. there are no hex or `rgb()` colours, gradients, shadows or animation keyframes;
3. text sizes come from the table.

Change the spec here and in `tokens.ts` together.
