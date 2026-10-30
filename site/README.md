# Project website

A small static site built with React and Fluent UI v9, using the repository's own dependencies and the launcher's brand theme (`src/lib/brand.ts`). It exists so the project has a public home: what it is, how it handles credentials, and what data it touches. Mojang's app review asks for a related website.

Pages (English at `/`, Chinese under `/zh/`): home, security model, privacy.

## Develop

From the repository root:

```
npm run site:dev      # http://localhost:5190
npm run site:build    # type-checks, then writes site/dist
npm run site:preview  # serves site/dist on :5191
```

## Deploy

The site is a static folder, so any static host works. It is currently on Cloudflare Pages (project `aether-launcher`):

```
npm run site:build
npx wrangler pages deploy site/dist --project-name aether-launcher --branch main
```

- `site/public/_headers` sets a strict Content-Security-Policy and caching. The page needs `script-src 'self'` and `style-src 'unsafe-inline'` (Fluent injects style rules at run time) and nothing else.
- With no `404.html`, Cloudflare Pages serves `index.html` for every unknown path, which is what the client-side router relies on.
- The canonical origin is `https://launcher.abnt.it` (`siteOrigin` in `src/router.ts`, `public/sitemap.xml`, `public/robots.txt`). Attach it under Workers & Pages, then `aether-launcher`, then Custom domains.

## Screenshots

`public/launcher-dark.webp` and `launcher-light.webp` are real captures of the launcher's interface, taken from the dev server running against its mock backend (`src/dev/mockBackend.ts`), so the instance names and the account in them are sample data. They are 1180 by 760 at 2x. Retake them when the interface changes noticeably: run `npm run dev -- --port 5192`, open it in a 1180 by 760 window in each theme (`localStorage["aether.theme"]` is `dark` or `light`), remove the "mock data" chip, and save as WebP. The launcher's interface is currently in Chinese only, which the alt text says.

## Keeping it honest

Everything on the site must match the launcher. When behaviour changes, update the status list in `src/pages/Home.tsx`, the network table in `src/pages/Privacy.tsx` and the rules in `src/pages/Security.tsx`, and bump `lastUpdated` in `src/router.ts`. The English and Chinese copy are written side by side in each page file; change both.

The site makes no network requests of its own: no analytics, no fonts, no third-party scripts. Keep it that way, because the privacy page says so.
