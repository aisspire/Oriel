# Oriel

面向个人独立工具的可配置工作台，通过插件与适配组织工具的交付、运行和展示。

当前为 **0.1 内置浏览器与网页接入初版**：保留插件一级侧栏、品牌首页与浅深双主题，通过浏览器二级侧栏管理固定网页页签，提供地址栏、上一页／下一页、刷新／停止、Windows 原生密码保存与登录填充、默认代理及网页覆盖。

## 开发运行

使用 Node.js 22.12 或更高版本，项目版本声明见 `.node-version`。使用 npm 与已提交的 `package-lock.json`：

```bash
npm ci
npm run dev
```

浏览器打开 `http://127.0.0.1:1420`。页面所需图标、样式与字体回退均在本地可用，不依赖远程图片或字体服务。

```bash
npm run build       # 类型检查与前端生产构建
npm run preview     # 预览生产构建
npm run icons       # 从主 SVG 重新导出深浅主题与桌面图标
```

## 桌面应用

桌面外壳沿用 Tauri 2 与 Rust。已具备对应平台开发环境时：

```bash
npm run tauri dev
npm run tauri build
```

在目标平台的开发环境中执行桌面构建；Windows 安装包请在 Windows 环境构建。系统前置条件见 [Tauri 官方说明](https://v2.tauri.app/start/prerequisites/)。项目不会自动安装系统库或更改全局工具链。

Windows 打包在项目根目录执行，默认生成 NSIS 安装包：

```powershell
.\build.cmd

.\build.cmd -Check                 # 只检查工具、版本和本地 CLI
.\build.cmd -InstallDependencies   # 先按锁文件执行项目级 npm ci，再打包
.\build.cmd -NoBundle              # 只生成 Release 程序
```

[build.cmd](build.cmd) 调用根目录的 [build.ps1](build.ps1)，不更改系统执行策略。脚本定位自身所在目录，可从其他工作目录调用；使用项目内 Tauri CLI，自动完成前端构建，并以 `--locked` 构建 Rust。缺少项目依赖时提示处理，默认不安装；不自动安装系统库或全局工具。

默认程序为 `target/windows/release/oriel.exe`，安装包位于 `target/windows/release/bundle/nsis/`。可用 `-TargetDir <目录>` 指定构建位置；未指定时优先沿用已有 `CARGO_TARGET_DIR`，否则使用 `target/windows`。脚本结束后恢复原环境变量与工作目录。首次打包可能需要 Tauri 下载 NSIS 工具，内置浏览器声明随安装包分发。

Windows 与 WSL 应分别安装各自平台的 `node_modules`；Windows 脚本使用 Windows Node.js 与 Rust。项目依赖使用同一份 `package-lock.json`，不升级依赖或重新生成锁文件。此前的 `target/windows-stage/` 为本地构建暂存目录；当前脚本直接构建工作区最新内容，构建产物不纳入版本管理。

## 本版范围

- 一级侧栏提供首页、浏览器和外部插件页面；浏览器通过单一固定页签列表添加、编辑、移除网页，支持拖拽排序及上移／下移，不区分常用与普通标签页。
- 浏览器二级侧栏支持展开、收窄、鼠标及键盘调宽，窄窗口自动使用图标模式；顺序和侧栏偏好保存在本地配置，切换插件或调整布局保留网页实例。
- 内置插件位于 `plugins/builtin/`；外部插件仓库可留在仓库之外，在“浏览设置”中登记包含 `plugin.json` 的目录。新增兼容网页声明无需重新编译。
- 网页使用独立原生 WebView，按入口隔离 Cookie、网站存储与密码，切换页面只隐藏视图。
- Windows 密码能力复用 WebView2 原生保存、更新提示及登录填充，可单独清除此入口保存的密码；不提供 Oriel 密码列表或读取个人浏览器密码库。
- 代理支持跟随系统、直连、HTTP 及 SOCKS5；网页可覆盖默认设置，修改代理保留登录数据并重建受影响实例。暂不支持代理账号密码。
- 宿主配置为应用配置目录中的 `workspace.json`（版本 2），兼容读取版本 1 并在成功保存时迁移，保留网页 ID、代理和浏览数据。外部网页不继承本地宿主命令权限。通用插件安装、更新和脚本执行继续延后。
- 首页只显示 Oriel 名称与动效，移除欢迎语、辅助卡片、工具空状态和页脚说明。
- Windows 窗口去除原生标题栏、外框和 1px 阴影边线；Windows 11 使用原生圆角，最大化和贴边时由系统处理为直角，提供拖动、最小化、最大化 / 还原与关闭操作。
- 首次启动窗口为 1100×760 逻辑像素，正常关闭后记忆大小、位置和最大化状态；显示器或缩放变化时限制到可用工作区。窗口偏好保存在应用配置目录的 `window-state.json`，异常终止不保证保存最后一次调整。
- 新黑白字标使用真实 SVG，侧栏、网页图标与运行窗口图标随深浅主题反转。
- 外观选择仅作为本地界面偏好保存；存储不可用时仍可切换当前页面外观。
- 动效可暂停，系统启用减少动态效果时自动静态展示；页面进入后台时暂停动画。
- 首页阶段曾构建 Windows x64 Release 和 NSIS 安装包；本轮浏览器的验证单独记录，不沿用旧构建结论。

## 浏览器验证

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml -p oriel-core --locked
```

在 Windows 构建调试程序后，可运行 `npm run check:browser`；默认程序路径为 `target/windows/debug/oriel.exe`，验收数据隔离保存在 `target/browser-check/`，需要端口 1420 可用。实际网站密码保存提示与再次填充仍需测试账号人工验收。

声明示例、配置归属、原生能力和验证边界见[网页接入初版](docs/design/web-page-plugins.md)。

2026-10-21 初版浏览器实现阶段曾通过前端类型检查与生产构建、12 项核心测试、16 项隔离 Windows 原生检查，并构建 Windows Release 程序和 NSIS 安装包，未执行安装程序。

同日固定页签布局阶段完成前端类型检查与生产构建、19 项核心测试、30 项隔离 Windows 原生检查及 Windows 调试程序构建；包含配置迁移、伸缩调宽、排序、目标页签菜单、长列表、六种视口和既有原生浏览行为。已审查展开、图标模式及深色窄屏截图。该布局尚未重新生成 Release 安装包；原生密码保存提示与再次填充仍待人工验收。

## 目录

```text
src/                 Vue 首页、组件、外观偏好与样式
public/              Oriel SVG 图标与触屏图标
src-tauri/           Tauri 桌面外壳、原生网页能力与平台图标
src-tauri/crates/    平台无关的声明、配置与校验
plugins/builtin/    内置浏览器声明与界面
docs/design/         当前设计、视觉说明与决策记录
```

- [文档导航](docs/README.md)
- [当前设计基线](docs/design/current-design.md)
- [首页与界面设计](docs/design/home-and-identity.md)
- [开发约定](AGENTS.md)
