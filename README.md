# Oriel

面向个人独立工具的可配置工作台，通过插件与适配组织工具的交付、运行和展示。

当前为 **0.1 首页外壳**：单侧边栏、默认首页、Oriel 应用图标与品牌动效，支持浅色 / 深色外观。首页仅展示 Oriel 名称与柔光动效，Windows 窗口采用无原生边框设计，尚未接入插件、业务功能或宿主管理能力。

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

仅生成 Windows NSIS 安装包时，在 Windows 项目目录执行：

```bash
npm run tauri -- build --bundles nsis -- --locked
```

默认产物位于 `src-tauri/target/release/`，其中 `oriel.exe` 可直接运行，安装包位于 `bundle/nsis/`。设置 `CARGO_TARGET_DIR` 后，产物改为写入指定目录。打包工具缓存保存在该构建目录的 `.tauri/` 中。

Windows 与 WSL 不共用 `node_modules`。本次 Windows 构建使用 `target/windows-stage/` 作为独立工作目录，并按同一锁文件安装项目依赖；产物位于 `target/windows/release/`。这些目录均为本地生成内容，不纳入版本管理。

## 本版范围

- 单侧边栏只提供首页入口；未添加不可用的工具、设置或插件入口。
- 首页只显示 Oriel 名称与动效，移除欢迎语、辅助卡片、工具空状态和页脚说明。
- Windows 窗口去除原生标题栏、外框和 1px 阴影边线；Windows 11 使用原生圆角，最大化和贴边时由系统处理为直角，提供拖动、最小化、最大化 / 还原与关闭操作。
- 新黑白字标使用真实 SVG，侧栏、网页图标与运行窗口图标随深浅主题反转。
- 外观选择仅作为本地界面偏好保存；存储不可用时仍可切换当前页面外观。
- 动效可暂停，系统启用减少动态效果时自动静态展示；页面进入后台时暂停动画。
- 已完成前端生产构建与类型检查，并于 2026-10-21 构建 Windows x64 Release 程序和 NSIS 安装包、启动桌面应用；按要求未运行测试。

## 目录

```text
src/                 Vue 首页、组件、外观偏好与样式
public/              Oriel SVG 图标与触屏图标
src-tauri/           最小 Tauri 桌面外壳与平台图标
docs/design/         当前设计、视觉说明与决策记录
```

- [文档导航](docs/README.md)
- [当前设计基线](docs/design/current-design.md)
- [首页与界面设计](docs/design/home-and-identity.md)
- [开发约定](AGENTS.md)
