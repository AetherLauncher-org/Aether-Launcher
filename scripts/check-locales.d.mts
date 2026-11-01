/**
 * Type declarations for `scripts/check-locales.mjs`, so `src/i18n` tests can
 * import the pure `checkCatalogs` function without pulling Node types into the
 * frontend tsconfig.
 */
export function checkCatalogs(
  catalogs: Readonly<Record<string, Readonly<Record<string, string>>>>,
): string[][];
