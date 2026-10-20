# Oriel

面向个人独立工具的可配置工作台，通过插件与适配组织工具的交付、运行和展示。

当前为 **0.1 首页外壳**：单侧边栏、默认首页、Oriel 应用图标与品牌动效，支持浅色 / 深色外观。工具空间为空，尚未接入插件、业务功能或宿主管理能力。

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
```

## 桌面应用

桌面外壳沿用 Tauri 2 与 Rust。已具备对应平台开发环境时：

```bash
npm run tauri dev
npm run tauri build
```

在目标平台的开发环境中执行桌面构建；Windows 安装包请在 Windows 环境构建。系统前置条件见 [Tauri 官方说明](https://v2.tauri.app/start/prerequisites/)。项目不会自动安装系统库或更改全局工具链。

## 本版范围

- 单侧边栏只提供首页入口；未添加不可用的工具、设置或插件入口。
- 首页展示 Oriel 动效和真实空工具状态，不包含示例业务数据。
- 外观选择仅作为本地界面偏好保存；存储不可用时仍可切换当前页面外观。
- 动效可暂停，系统启用减少动态效果时自动静态展示；页面进入后台时暂停动画。
- 本轮已完成前端生产构建与类型检查，按要求未运行测试；未运行桌面应用或构建桌面安装包。

## 目录

```text
src/                 Vue 首页、组件、外观偏好与样式
public/              Oriel SVG 图标与触屏图标
src-tauri/           最小 Tauri 桌面外壳与平台图标
docs/design/         当前设计、视觉说明与决策记录
```

- [文档导航](docs/README.md)
- [当前设计基线](docs/design/current-design.md)
- [初版首页与视觉设计](docs/design/home-and-identity.md)
- [开发约定](AGENTS.md)
