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
- Sidebar navigation between home, instances, library, accounts, and settings
- Typed frontend IPC layer (`src/ipc`)
- Architecture overview in `docs/architecture.md`
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
