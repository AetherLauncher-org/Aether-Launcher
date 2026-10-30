# Changelog

All notable changes to Aether Launcher will be documented in this file.

The project follows [Semantic Versioning](https://semver.org/) once public releases begin.

## [Unreleased]

### Added

- Rust workspace with separated launcher domain crates
- Tauri 2 desktop shell
- React, TypeScript, and Fluent UI v9 interface foundation
- Frontend lint, tests, and production build
- Rust formatting, linting, and unit-test checks
- Issue Forms, Discussion Forms, pull request template, and CODEOWNERS
- Dependabot, CodeQL, dependency review, labeling, CI, and draft release workflows
- Navigation between home, instances, download, accounts, and settings; the navigation pane collapses to icons
- Typed frontend IPC layer (`src/ipc`)
- Architecture overview in `docs/architecture.md`
- Project website in `site/` (React + Fluent UI v9, English and Chinese: home, security model, privacy), deployed to Cloudflare Pages
- Install Forge, NeoForge and Fabric from the version page (the version is installed first, then the loader, and the instance is created last); every loader download is checked against the SHA-1 published next to it on the official maven, and new instances start in Chinese with the accessibility wizard skipped
- Microsoft sign-in (device code flow): refresh token in the OS credential vault, no token ever crosses IPC, launch as a signed-in account, `login://status` events; switched off until an Azure application id is configured (`AETHER_MS_CLIENT_ID`) and approved by Mojang (see `docs/modules/microsoft-login.md`)
- Browser dev preview runs against a mock backend (`src/dev/mockBackend.ts`, dev server only; `?mock=off` disables it)
- Memory slider snaps to 512 MiB (hold Shift for fine steps), shows tick marks, and no longer owes the pointer overshoot at either end
- Import from PCL2, HMCL, BakaXL, Prism Launcher / MultiMC or a plain `.minecraft`: versions are installed into the launcher-owned game tree from the official manifest, reusing hash-matching files from the source folder as a read-only seed; loader versions are skipped
- One-click Vanilla install (`install_vanilla_version`): official catalog, verified client JAR, libraries, native archives and assets, then a ready instance; already-verified files are skipped, progress arrives as `install://progress` events, and the install can be cancelled
- Launch uses the launcher-owned game directory, an optional account UUID, and a `game_status` query so the UI notices when the game exits
- Frameless window with a custom title bar, Mica material on Windows 11, Fluent 2 design tokens, a Fluent motion system with a reduced-motion switch, and a new Aether logo and icon set
- Download page that browses every official version with its metadata before installing, replacing the cache, download and create buttons
- Offline accounts added through a dialog with standard, random or custom UUIDs, kept across restarts
- Memory setting sized from the machine: an integrated slider whose track is the installed memory, with system use and the game's share on it, plus an automatic mode that recomputes from the free memory at every launch; system_memory reads it
- Live smoke tests (ignored by default) that install and launch a real version
- Appearance setting with dark, light, and follow-system colour schemes
- Version manifest parsing for libraries, arguments, asset index, client download and Java version, with rule evaluation
- `inheritsFrom` merging: child libraries first, newest version wins among identical coordinates, parent arguments first
- Microsoft device code login with the Xbox Live, XSTS and Minecraft token chain, plus typed XSTS error codes
- Offline account UUID derived with the Java offline player UUID algorithm, together with player-name validation
- Credential store trait with an atomic file implementation that keeps access tokens off disk
- Resumable download engine: Range continuation, bounded retry with backoff, multi-source fallback with per-provider circuit breaking, progress reporting and cancellation
- Streaming SHA-1 computed while writing, with artifacts published through a `.part` file and an atomic rename
- Fabric, Quilt, Forge and NeoForge metadata parsing with install plans built from injected metadata sources
- `aether-diagnose` crate: launch evidence collection, failure stage determination, local crash classification rules, JVM argument preflight table and a cache signature
- Crash analysis first-layer redaction: launch arguments by position, user names in paths, player names, server addresses, and email detection that ignores `module@version` strings
- Instance creation, atomic record updates and a fault-tolerant scan over the managed instance store
- PCL2 and HMCL detection with import plans that fill only missing files
- Editable instance configuration for memory, Java override, JVM arguments, game arguments and window settings
- `os.version` rules evaluated as regular expressions
- Instance, library and account pages with Java runtime and memory settings blocks

### Security

- Crash analysis runs locally first and displays the redacted payload to the user before anything is sent off the machine
- Refresh token persistence is opt-in and disabled by default; the store is atomic but not encrypted, and its protection comes from the application data directory's permissions
- Instance commands accept typed identifiers only, with the storage root directory resolved by the backend rather than supplied by the caller

### Changed

- Home greeting follows the local time of day
- Document language set to `zh-CN`
- Download plans reject unsafe or duplicate destination paths
