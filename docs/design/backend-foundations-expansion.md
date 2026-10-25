# 后端基础扩展（与 PR #12 不重合的部分）

本文件是分支 `feat/backend-foundations-expansion` 的边界声明。PR #12
（`feat/java-runtime-discovery`）已经实现 Java 发现、版本清单加载与继承、下载校验与
natives 安装、启动计划组装、进程监督、实例进程注册表、可信实例读取与启动协调。

本分支只做 PR #12 **没有**实现的能力，且不修改 PR #12 已经写过的文件：

| 模块                                        | 负责内容                                                                 | 归属   |
| ------------------------------------------- | ------------------------------------------------------------------------ | ------ |
| `aether-auth::{device_code,xbox,minecraft}` | 微软设备码登录、XBL 与 XSTS 换取、Minecraft 令牌与 profile 获取          | 本分支 |
| `aether-auth::offline`                      | 离线账户 UUID 派生                                                       | 本分支 |
| `aether-auth::store`                        | 凭据存储：落盘加密判定与凭据读写边界                                     | 本分支 |
| `aether-auth::token`                        | 令牌脱敏 newtype，让误打印令牌变难                                       | 本分支 |
| `aether-download::engine`                   | 真实 HTTP 传输、分块续传、重试退避、多源回退、进度与取消                 | 本分支 |
| `aether-modloader`                          | Fabric / Quilt / Forge / NeoForge 元数据解析与加载器安装计划             | 本分支 |
| `aether-diagnose`（新 crate）               | 崩溃证据采集模型、失败阶段判定、本地规则分类、第一层脱敏、服务端 payload | 本分支 |
| `aether-storage`                            | 实例创建与原子写入、实例列表扫描、PCL2 / HMCL 实例导入                   | 本分支 |
| `aether-manifest`                           | `logging` 字段、`os.version` 正则规则                                    | 本分支 |
| `aether-core`                               | 实例可编辑配置（内存、Java 覆盖、JVM/游戏参数、窗口大小）                | 本分支 |
| 前端                                        | 实例页、资源库页、账户页、设置的 Java / 内存区块、带参数的 IPC 调用      | 本分支 |

> 表里的 `logging` 字段仍是规划项，尚未在 `aether-manifest` 落地；同一行的 `os.version` 正则规则已实现。

## 硬性约束

1. **网络 IO 全部藏在 trait 后面。** 任何测试都不允许访问真实网络。HTTP 传输以
   `Transport` trait 表达，测试使用内存替身。原因是 CI 没有凭据、也不允许访问
   Mojang / 微软接口。
2. **不把新的运行时 HTTP 依赖放进单元测试路径。** 若需要 HTTP 客户端，只作为
   `aether-auth` / `aether-download` 的默认实现单独成模块，且必须能被 trait 替换。
3. **日志、令牌、路径都是不可信输入。** 错误信息里不得出现 access token、refresh
   token、完整 argv 或用户目录之外的绝对路径。
   路径本身允许出现，但其中的用户名必须替换为占位符（`C:\Users\<名字>\`、`/home/<名字>/`）。
4. **不新增可执行任意命令或读写任意路径的 Tauri IPC。** 新增命令只接受已注册的实例
   ID 或后端配置的根目录。
5. **每个行为变化都要有 Rust 单元测试或前端测试。** 文件超过 400 行时拆成子模块，
   测试放在 `<module>/<module>_tests.rs`（与 PR #12 的风格一致）。

## 代码审查自查清单

下表把上面的约束落成可核对的条目。通过标准一列写的是能在 diff 里直接看到的证据，不接受口头说明。

| 检查项                             | 通过标准                                                                                                   | 常见失败                                                        |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| 网络 IO 是否在 trait 后面          | 新增的 HTTP 调用只出现在 trait 的默认实现或专属 `http` 模块里；业务模块的 `use` 列表里没有 HTTP 客户端类型 | 在业务函数里直接 `reqwest::get`，替身只覆盖成功路径             |
| 测试是否发起真实请求               | 测试文件里没有真实域名，`Transport` 替身按调用计数返回预设结果                                             | 真实请求留着再 `#[ignore]`；只测成功路径，失败分支靠手测        |
| HTTP 依赖是否进单测路径            | HTTP 客户端是可选的 `feature` 或独立模块，`cargo test -p aether-auth --no-default-features` 能编译通过     | 把客户端塞进 `[dependencies]`，再在纯逻辑模块里 `use`           |
| 错误信息与日志是否脱敏             | 错误枚举与日志宏的参数里没有承载令牌的 `String`、完整 argv、路径里的用户名字段没有替换                     | `#[error("{0:?}")]` 打出整个响应体；调试期留下的 `debug!`       |
| IPC 是否窄化                       | 新命令签名只有实例 ID、枚举或标量配置，根目录由后端解析                                                    | 签名出现 `path: String`、`command: String`、`args: Vec<String>` |
| 是否绕过路径校验                   | 所有写盘路径都经过下载计划或 storage 的同一套校验函数                                                      | 给「临时文件」另开一条不校验的路径                              |
| 是否有测试且命名符合约定           | 每个行为变化对应一个测试；新增测试文件名为 `<module>/<module>_tests.rs`                                    | 新模块 600 行、0 个测试；测试塞进 `lib.rs` 的 `mod tests`       |
| 失败是否可解释                     | 失败由错误枚举的分类变体表达，错误链里带阶段与重试次数                                                     | `Err("failed".into())`；把上游错误字符串原样上抛                |
| 文档是否同步更新                   | 用户可见行为变化在同一 commit 里改了 `CHANGELOG.md` 与 `docs/` 对应页面                                    | 只改代码，文档留到以后                                          |
| 前端 `call` 封装与命令签名是否一致 | `CommandArgs` / `CommandResults` 里新增的键与 `#[tauri::command]` 函数名、参数一一对应，`tsc` 无报错       | 后端改了参数名，前端仍传旧键名                                  |

`docs/modules/` 下三篇模块文档各有一节「不变量」，与本表冲突时以本表为准。

## 合并冲突回避

`src-tauri/src/lib.rs`、`src/ipc/index.ts`、`src/ipc/index.test.ts`、
`docs/architecture.md`、`Cargo.toml`、`Cargo.lock` 与 PR #12 重叠，由主 lead 在集成
阶段统一处理，各功能团队不直接编辑这些文件（`Cargo.toml` 与 `Cargo.lock` 在脚手架
阶段一次性加好）。合并步骤不在这里重复，见下文「与 PR #12 的文件重叠与合并顺序」。

## 与 PR #12 的文件重叠与合并顺序

这些文件在两边都按同一位置追加内容，git 无法判断该保留哪一侧，不能靠自动合并。

| 文件                    | 重叠原因                                                                                          | 处理方式                                              |
| ----------------------- | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `src-tauri/src/lib.rs`  | 两边都往 `generate_handler!` 列表里加命令                                                         | 人工合并列表，确认命令名唯一、参数类型与前端登记一致  |
| `src/ipc/index.ts`      | 两边都在 `CommandResults` 里登记命令与返回类型，并加封装函数                                      | 人工合并，先并类型再并封装函数，`call` 扩参一次改到位 |
| `src/ipc/index.test.ts` | 两边都新增命令测试，`invoke` 桩要跟着补；命令测试目前分散在 `src/ipc/` 下的多个 `*.test.ts`，合并时按实际情况处理 | 人工合并，确认桩覆盖新命令，不只删冲突标记            |
| `Cargo.lock`            | 两边都新增依赖                                                                                    | 不手工合并，合并后重新生成                            |
| `docs/architecture.md`  | 两边都改 crate 表与「还没有的部分」                                                               | 由主 lead 按合并后的真实状态改写，功能团队不直接编辑  |
| `Cargo.toml`（工作区）  | 两边都加成员 crate 与依赖条目                                                                     | 脚手架阶段一次性加好，功能团队只读不写                |

建议的合并顺序：

1. 先合并各功能 crate 代码，让 `cargo check --workspace` 本地通过。
2. 再让 `Cargo.lock` 重新生成（不手工解决 lock 冲突）。
3. 再手工合并 `src-tauri/src/lib.rs` 的 `generate_handler!` 列表（确认命令名唯一、参数类型正确）。
4. 接着合并 `src/ipc/index.ts` 与 `index.test.ts`（先并类型再并封装函数，`call` 扩参一次改到位）。
5. 最后统一文档（`docs/architecture.md` 由主 lead 改写，`CHANGELOG.md` 与 `docs/modules/` 措辞一致）。

为什么是这个顺序：前三步决定代码能否编译，后两步决定前端能否调用、文档是否对得上。顺序颠倒的话，文档会先写进最终并不存在的命令，前端封装也会对着还没合并的签名改一遍再返工。
