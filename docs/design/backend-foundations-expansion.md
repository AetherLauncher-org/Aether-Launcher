# 后端基础扩展（与 PR #12 不重合的部分）

本文件是分支 `feat/backend-foundations-expansion` 的边界声明。PR #12
（`feat/java-runtime-discovery`）已经实现 Java 发现、版本清单加载与继承、下载校验与
natives 安装、启动计划组装、进程监督、实例进程注册表、可信实例读取与启动协调。

本分支只做 PR #12 **没有**实现的能力，且不修改 PR #12 已经写过的文件：

| 模块 | 负责内容 | 归属 |
| --- | --- | --- |
| `aether-auth` | 微软设备码登录、XBL/XSTS/Minecraft 令牌链、离线账户 UUID、凭据存储 | 本分支 |
| `aether-download::engine` | 真实 HTTP 传输、分块续传、重试退避、多源回退、进度与取消 | 本分支 |
| `aether-modloader` | Fabric / Quilt / Forge / NeoForge 元数据解析与加载器安装计划 | 本分支 |
| `aether-diagnose`（新 crate） | 崩溃证据采集模型、失败阶段判定、本地规则分类、第一层脱敏、服务端 payload | 本分支 |
| `aether-storage` | 实例创建与原子写入、实例列表扫描、PCL2 / HMCL 实例导入 | 本分支 |
| `aether-manifest` | `logging` 字段、`os.version` 正则规则 | 本分支 |
| `aether-core` | 实例可编辑配置（内存、Java 覆盖、JVM/游戏参数、窗口大小） | 本分支 |
| 前端 | 实例页、资源库页、账户页、设置的 Java / 内存区块、带参数的 IPC 调用 | 本分支 |

## 硬性约束

1. **网络 IO 全部藏在 trait 后面。** 任何测试都不允许访问真实网络。HTTP 传输以
   `Transport` trait 表达，测试使用内存替身。原因是 CI 没有凭据、也不允许访问
   Mojang / 微软接口。
2. **不把新的运行时 HTTP 依赖放进单元测试路径。** 若需要 HTTP 客户端，只作为
   `aether-auth` / `aether-download` 的默认实现单独成模块，且必须能被 trait 替换。
3. **日志、令牌、路径都是不可信输入。** 错误信息里不得出现 access token、refresh
   token、完整 argv 或用户目录之外的绝对路径。
4. **不新增可执行任意命令或读写任意路径的 Tauri IPC。** 新增命令只接受已注册的实例
   ID 或后端配置的根目录。
5. **每个行为变化都要有 Rust 单元测试或前端测试。** 文件超过 400 行时拆成子模块，
   测试放在 `<module>/<module>_tests.rs`（与 PR #12 的风格一致）。

## 合并冲突回避

`src-tauri/src/lib.rs`、`src/ipc/index.ts`、`src/ipc/index.test.ts`、
`docs/architecture.md`、`Cargo.toml`、`Cargo.lock` 与 PR #12 重叠，由主 lead 在集成
阶段统一处理，各功能团队不直接编辑这些文件（`Cargo.toml` 与 `Cargo.lock` 在脚手架
阶段一次性加好）。
