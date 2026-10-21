<div align="center">

# Aether Launcher

**透明、可靠、可迁移的 Minecraft Java 版实例管理器**

[![CI](https://github.com/AetherLauncher-org/Aether-Launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/AetherLauncher-org/Aether-Launcher/actions/workflows/ci.yml)
[![CodeQL](https://github.com/AetherLauncher-org/Aether-Launcher/actions/workflows/codeql.yml/badge.svg)](https://github.com/AetherLauncher-org/Aether-Launcher/actions/workflows/codeql.yml)
[![License](https://img.shields.io/github/license/AetherLauncher-org/Aether-Launcher)](LICENSE)
[![GitHub Release](https://img.shields.io/github/v/release/AetherLauncher-org/Aether-Launcher?include_prereleases)](https://github.com/AetherLauncher-org/Aether-Launcher/releases)

<img src="https://ld.xh.do/ld-badge.svg" alt="认可linux.do">

</div>

> [!IMPORTANT]
> Aether Launcher 目前处于早期开发阶段。请不要把当前版本当作可替代日常启动器的发行版。

## 为什么做 Aether Launcher

因为不可抗力的原因，部分主流启动器不得不关闭或限制了下载功能。Minecraft 实例不应该被某一个启动器、下载源或服务入口锁死。

Aether Launcher 的目标是提供一个**不依赖单一中心化服务**的替代方案：
- **本地优先**：完美兼容并导入你现有的游戏实例，支持从 PCL2 / HMCL / BakaXL / Prism Launcher / Modrinth / ... 一键导入，只补齐缺失文件
- **多源容错**：不绑定单一下载源，记录每个文件的来源与散列，随时可审计
- **纯净开源**：不搞小动作，代码完全公开，数据完全保存在本地
- **跨平台**：Rust 核心，首发 Windows，后续支持 macOS 与 Linux

我们支持 Linux.DO 社区与开放、友善、重视技术质量的开发者社区。

## 技术栈

```text
Rust workspace
├── Tauri 2                 桌面
├── aether-core             核心
├── aether-manifest         Minecraft 版本元数据
├── aether-download         下载、来源与校验
├── aether-auth             账户与认证
├── aether-runtime          Java 管理
├── aether-launch           游戏启动
├── aether-modloader        Mod Loaders
└── aether-storage          本地数据存储

React + TypeScript
├── Fluent UI React v9      Fluent 2 组件与设计语言
├── Vite                    前端构建
└── Vitest                  前端测试
```

## 本地开发

### 环境要求

- Node.js 24 或当前维护中的 LTS
- npm 11+
- Rust stable
- Tauri 2 对应的平台构建依赖

Windows 需要 Visual Studio Build Tools 的“使用 C++ 的桌面开发”工作负载。Linux 需要 WebKitGTK 4.1 等 Tauri 系统依赖。

### 启动与检查

```powershell
npm ci
npm run tauri dev
```

提交前运行：

```powershell
npm run check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

仅检查 Rust 核心：

```powershell
cargo test --workspace --exclude aether-desktop
```

## 当前里程碑
见 Milestone。

具体需求见 [Issues](https://github.com/AetherLauncher-org/Aether-Launcher/issues)，设计讨论和问答见 [Discussions](https://github.com/AetherLauncher-org/Aether-Launcher/discussions)。
提交前请确保没有重复 Issue 或话题。

## 文档

- [架构概览](docs/architecture.md)：crate 分层、前端结构、IPC 边界，以及各模块还缺什么
- [模块文档](docs/modules/README.md)：账户、下载引擎与实例生命周期的实现边界

## 贡献

开始编码前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。安全问题请按照 [SECURITY.md](SECURITY.md) 使用 GitHub 私密漏洞报告。

## 不接受赞助

Aether Launcher 不接受项目赞助。如果你原本打算赞助本项目，可以考虑把这份心意交给真正需要帮助的人：

- [腾讯公益](https://gongyi.qq.com/succor/project_list.htm)
- [支付宝公益](https://love.alipay.com/donate/index.htm)
- [壹基金](https://onefoundation.cn/donate/)
- [GlobalGiving](https://www.globalgiving.org/)
- [GoFundMe](https://www.gofundme.com/)
- [DonorsChoose](https://www.donorschoose.org/)

以上链接仅作为公益捐赠入口参考，不代表 Aether Launcher 与这些平台存在合作、隶属、授权或背书关系。

## 免责声明

NOT AN OFFICIAL MINECRAFT SERVICE. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
Aether Launcher 是独立的社区项目，与 Mojang Studios、Microsoft、Minecraft 或 Linux.DO 不存在官方隶属、授权或背书关系。“Minecraft”是其权利人的商标。

## 许可证

代码以 [Apache License 2.0](LICENSE) 发布。
