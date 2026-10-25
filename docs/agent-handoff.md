# 交接：PR #12 / #13 / #21 与凭据存储

给接手的 agent（包括新开对话的 GPT、GLM、DeepSeek 或 Claude）。先读这份，再读 [协调帖 #22](https://github.com/AetherLauncher-org/Aether-Launcher/issues/22)。

本文件写于 `main` 的 `a4f4842`。每条事实后面注明是怎么核实的。之后的状态以 GitHub 为准，改动前先重新核对。

## 分支与 SHA（2026-10-25 核对）

| 对象 | 分支 | 头提交 | 状态 |
| --- | --- | --- | --- |
| #12 | `feat/java-runtime-discovery` | `966e91e` | draft，GPT 写，约 5 千行 |
| #13 | `feat/backend-foundations-expansion` | `f583166` | draft，agent 团队写，约 2.8 万行 |
| #21 | `integration/pr12-pr13` | `294a462` | draft，GPT 集成，CI 全绿 |
| #14–#20 | `feat/*`（按 crate 拆分 #13） | 见 PR 列表 | draft，由 Claude 机械拆分，叠在 #14 上 |
| 本地 | `feat/desktop-instance-ui` | 未推送 | 桌面壳和前端的拆分第 8 步，改动已存入 `git stash` |

核实：`git rev-parse` 和 `gh pr view <n> --json headRefOid`。

## 已确认的事实

- **#21 上没有任何代码能把刷新令牌写到磁盘。** 磁盘写入只有 `crates/aether-auth/src/store.rs` 的 `write_atomically`，它写的是 `WireCredential` 的 JSON。`from_credential` 把 `refresh_token` 固定为 `None`。`FileCredentialStore::save` 在 `Allow` 且有令牌时返回 `InsecurePersistence`。核实：读源码，`git show origin/integration/pr12-pr13:crates/aether-auth/src/store.rs`。
- **`RefreshTokenPersistence::Allow` 仍是公开变体，但是死代码。** 它不写文件，只会返回错误。建议删除，由编译器确认没有调用方。
- **`KeyringCredentialStore`（`secure_store.rs`）是 fail-closed 的。** 所有 keyring 错误都映射为 `ProtectedStoreUnavailable`，没有回退到明文的分支。
- **`secure_store.rs` 的测试没有覆盖 `save`、`load`、`remove`。** 只有两个测试，断言构造函数和常量。核实：读 `#[cfg(test)]` 部分。这一条是 #22 里的阻塞项。
- **CI 绿不等于保险库行为正确。** CI 上的 Linux runner 通常没有 Secret Service，而且测试没有调用保险库。
- **`keyring = "=4.2.0"`，默认启用 `v1` feature**：依赖按 `target` 分段，只在对应平台编译。核实：GPT 在 #22 引用的 `docs.rs/crate/keyring/4.2.0/source/Cargo.toml.orig`。Linux 上是 zbus 的 Secret Service 后端。**没有证明**无 D-Bus 环境下的失败时间和是否会阻塞。
- **`Cargo.lock` 与 `main` 的关系**：`Cargo.lock` 在 `main` 之后有改动，合并前需要重新生成。

## 有证据的错误结论（不要照用）

- **GLM 5.3 在 #22 引用了 `e888bd8` 和「远端 force-update」。** 这两件事都不成立：该 SHA 不在本地历史里，#13 的远端 reflog 只有一次 fetch，头仍是 `f583166`。它审查的 `secure_store.rs` 只存在于 #21，不在 #13 上。以后引用 SHA 前先 `git cat-file -t` 确认。

## 未决问题（需要人决定，不要 agent 自行决定）

1. **合并策略。** `main` 的 ruleset `Main` 是 active 状态。GPT 在 #22 写明它要求：签名提交、线性历史（只允许 rebase 合并）、必要的审批，以及 last-push 审批。#21 含一个双父合并提交，与线性历史冲突。要选：合并 #21 后关闭 #12 和 #13，还是按 crate 顺序逐个合并拆分分支。
2. **拆分分支是否替代 #13。** GPT 认为它们只是审查用的模块分支，不替代 #13 或 #21。维护者未确认。
3. **`CredentialStore` 的签名是否改为「只存取元数据」。** DeepSeek 原本建议在 trait 签名上锁死。Claude 的意见是不改签名，靠实现保证。GLM 的意见是给 `Entry` 抽一个 `VaultEntry` 小 trait，便于用内存实现测试。这一条两人意见不同，需要决定。
4. **`StoreError::CorruptedCredential`。** GLM 建议区分「保险柜打不开」和「条目内容损坏」。GPT 同意，可作为 P2。

## 当前待办（按优先级）

**P1，阻塞合并**
- 给 `KeyringCredentialStore` 补测试，用内存后端（建议 `VaultEntry` trait），覆盖：令牌存取往返、`NoEntry` 返回 `None`、`save(None)` 删除条目、`remove` 在保险库失败时保留元数据、保险库锁定或不可用时不回退明文、离线账户不访问保险库、空令牌或格式错误的报错。
- 删除 `RefreshTokenPersistence::Allow`，同时修改调用方和测试，放在一个专门的 PR 里。

**P2**
- 增加 `StoreError::CorruptedCredential`。
- 确认 Linux 无 Secret Service 时的行为（有界超时还是阻塞），用真实环境做冒烟测试。
- 在文档 `docs/modules/auth.md` 写明：无保险库的环境下，微软账户仅在会话内有效。

**其他**
- 下载引擎的真实 HTTP 传输实现（`aether-download::engine` 的 `Transport` trait 已就绪，缺生产实现和注入式测试）。GLM 团队提议认领，分支基于 `integration/pr12-pr13`，不能引入任意 URL 的 Tauri IPC。
- 前端启动 UI（`src/ipc/launch.ts`）。归属未定。
- 桌面壳和前端的拆分第 8 步：在 `feat/desktop-instance-ui` 上，改动见 `git stash list` 里的 `desktop-instance-ui WIP`。Rust 侧 fmt 和 clippy 已通过，前端 `npm run check` 通过（167 个测试、生产构建），但 `npm ci` 需要强制使用官方源（见下）。

## 共享文件与规则

- 不要同时修改这些文件：`Cargo.lock`、`Cargo.toml`、`src-tauri/src/lib.rs`、`src/ipc/index.ts`。改之前先在 #22 对齐。
- 不要 force-push，不要改写已有分支。
- 不要直接推 `main`。
- 每条结论要带证据：`path:line`、测试输出，或 CI 的 job 名。
- 不要把 CI 绿色当成保险库行为已验证。

## 环境注意事项

- `package-lock.json` 里 354 个下载地址是 `registry.npmmirror.com`。某些环境连不上它，需要用 `npm ci --registry=https://registry.npmjs.org/ --replace-registry-host=always`，完整性哈希仍然会校验。
- Rust 工具链：本地使用 conda-forge 的 rust 1.99 手动安装，只存在于那个容器里。CI 使用 `dtolnay/rust-toolchain@stable`。

## 谁在做什么

- **Claude**：独立只读审查，负责 `aether-auth` 的 Track A 复核和 #14–#20 拆分分支的维护。不改集成分支，不推送 `main`。
- **GPT**：集成、CI，以及保险库抽象和它的 mock 测试。
- **GLM 5.3（ZCode）**：Track B 的默认 HTTP 传输，前端启动 UI 待对齐。
- **DeepSeek**：#22 中的 P1 建议由 GLM 接手讨论，DeepSeek 的发言尚未在 #22 上出现。
- **维护者（Aether-254）**：合并策略、拆分分支去留，以及上面「未决问题」里的决定。
