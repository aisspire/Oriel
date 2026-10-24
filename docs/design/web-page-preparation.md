# 网页前置程序

2026-10-24：新增按网页独立执行的本地前置程序，保留原有[本地服务与规则引用](local-services.md)。本文是本次配置和输出约定，不恢复已撤回的 script-events-v1，也不建立通用脚本插件或资源依赖图。Windows 为产品目标平台；本轮实际验证见文末。

## 使用与职责

在网页设置中启用“前置程序”，选择本地程序或脚本；保存、目录发现和重新加载接入不执行程序。首次打开关联网页，或明确选择“重新准备并加载”时执行。没有前置程序的网页保持原行为。

Oriel 管理执行实例、输出、准备结果和实际拥有的进程树；脚本封装具体工具差异。用户可以让脚本调用其他插件或独立工具的公开接口，缺少工具、解释器或接口时正常报错。工具负责自己的连接或服务管理，界面操作和脚本调用可以复用同一业务能力。脚本取得调用结果不自动取得外部工具资源的停止权。

每个网页独立执行。两个网页调用同一程序也不由宿主合并；被调用工具仍可自行复用资源。后台显示的是进程与执行结果，不持续检查端口、服务业务健康或自动重连。

## 配置与兼容

工作区配置版本为 4；`webEntries` 的每个网页可选增加：

```json
{
  "id": "user.dashboard",
  "title": "管理页面",
  "kind": "web-page",
  "url": null,
  "proxy": {"mode":"direct"},
  "preparation": {
    "path": "C:\\tools\\prepare.ps1",
    "runner": "auto",
    "args": [],
    "mode": "once",
    "timeoutSeconds": 120
  }
}
```

- `runner`：`auto`、`direct`、`cmd`、`powershell`、`custom`。自动模式按扩展名选择：`.exe` 直接执行，`.cmd`／`.bat` 使用 CMD，`.ps1` 使用 Windows PowerShell；不识别的类型须显式选择执行方式。终端窗口与解释器分开，首版后台托管，不提供输入交互。
- `path`：本地网页必须使用绝对路径。可选 `cwd` 也须为绝对路径，省略时使用程序所在目录。允许先保存尚未安装的本地程序，执行时再诊断缺少项。
- `args`：字符串数组，按参数传入。界面每行一个参数，不额外添加包围引号；空白不自动裁剪。CMD 使用独立执行适配和关闭延迟展开，参数内含双引号时明确拒绝，改用其他解释器或包装脚本。直接执行不隐式经过 Shell。
- `mode`：`once`（默认）或 `persistent`。`timeoutSeconds` 默认为 120，范围为 1–3600，限制准备阶段及一次性任务退出等待；持续程序已就绪后不要求定期输出。
- `runner: "custom"` 时增加 `interpreter: {"program":"node.exe","args":[]}`。执行顺序为解释器、解释器参数、脚本路径、程序参数。其他执行方式不接受 `interpreter`。

PowerShell 默认使用 `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File`，仅设置本次进程的执行策略；需要 PowerShell 7 时选择自定义解释器并指定 `pwsh.exe`。不安装解释器、不修改全局策略。

外部插件使用声明版本 3，在 `entries[]` 的 `web-page` 入口增加相同字段，能力仍为 `web-page`。插件程序和工作目录可相对插件目录解析；解析后的路径必须存在且位于插件目录内，越界符号链接拒绝。无效声明产生诊断，不阻止其他有效插件使用。

版本 1／2 插件仍保持既有语义。旧 `service`、`prerequisites`、`lifecycle` 不自动转换或删除；同一入口不能同时配置新前置程序与旧服务／规则引用。浏览器工作区入口自身不能声明前置程序，各网页分别配置。

工作区版本 1／2／3 只在内存中迁移，首次成功保存才写为 4，保留稳定 ID、网页顺序、代理、主题、插件目录和浏览数据。异常配置保留原文件并阻止覆盖。旧版 Oriel 不能读取版本 4，避免忽略前置步骤直接加载网页。

## 输出与完成

标准输出只输出一条 **UTF-8、以换行结束、不超过 64 KiB** 的 JSON 结果，不混入进度、横幅或子程序普通输出；普通日志写标准错误。包装脚本负责等待、收集和翻译子脚本的输出。

```json
{"type":"result","success":true,"url":"http://127.0.0.1:18080/"}
```

```json
{"type":"result","success":false,"error":"具体失败原因"}
```

成功可以省略 `url`，此时使用网页登记地址；两者都没有有效地址时准备失败。地址沿用 HTTP／HTTPS 校验，拒绝宿主内部地址、含账号密码的 URL 和其他协议。返回地址只用于本次运行，不覆盖登记地址、入口 ID 或浏览数据目录。未知字段、重复结果、非 JSON、无效 UTF-8 或未换行结果均诊断。

一次性任务必须返回成功结果，并以退出码 0 结束，才算准备完成。只输出成功却不退出仍受准备超时限制。任务结束后回收其残留子进程；不能用一次性任务偷偷遗留由宿主启动的服务器。

持续程序返回成功时允许加载网页，入口继续前台运行，后续不输出第二条准备结果。该入口退出（包括退出码 0）记为持续程序失败，保留已经打开的网页，不自动重启。持续服务应由入口维持，或调用独立工具的公开接口后以一次性任务结束，由工具管理外部资源。

结果输出失败、启动失败、异常退出、超时和日志分别可见。每次运行最多保留 256 条日志，每条最多 4 KiB，结果与日志不持久化为用户配置。

## 生命周期与网页

同页并发准备合并为一个任务；不同页不共用执行实例。准备等待不占用网页展示锁，也不阻止切换到其他页面。每次运行带独立代次，旧配置或旧运行的结果不得导航当前页面。

| 操作 | 行为 |
| --- | --- |
| 首次打开 | 准备成功后按返回地址或登记地址加载 |
| 切换、隐藏、调宽及普通刷新 | 不重复执行、不停止程序；刷新只作用于网页 |
| 重新准备并加载 | 清理该页旧托管程序，重新执行；相同地址也刷新已有网页 |
| 编辑、保存或重新加载配置 | 不执行新程序、不自动替换旧程序；变更的结果失效，显式重新准备后应用 |
| 显式停止 | 停止所属进程树，保留网页及诊断；不停止被调用工具独立拥有的资源 |
| 移除网页 | 提示并清理该页托管程序，再移除入口；不能确认清理时保留入口 |
| 完整退出 | 拒绝新启动、取消等待、关闭网页，再清理托管程序和旧服务接入 |

Windows 入口先挂起创建，加入带 `KILL_ON_JOB_CLOSE` 的独立 Job 后恢复，防止子进程逃过初始归属。停止直接终止所属 Job 并检查活动进程归零，不新增脚本停止协议。清理等待上限为 10 秒；无法确认时诊断并阻止重叠启动，不通过 PID、端口或进程名批量清理外部程序。

## 最小脚本示例

下列一次性模板只演示结果输出。真实准备命令、退出码检查或工具接口调用应放在成功输出之前；子程序输出应收集或转到标准错误。

CMD（脚本正文使用 ASCII；如需非 ASCII，明确配置编码）：

```bat
@echo off
rem Run preparation here and check its result before reporting success.
echo {"type":"result","success":true,"url":"http://127.0.0.1:18080/"}
exit /b 0
```

PowerShell（输出 UTF-8；Windows PowerShell 含中文源码时保存为 UTF-8 BOM）：

```powershell
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
try {
    # Run preparation here. Capture child stdout or redirect it to stderr.
    # Native program failures also require checking $LASTEXITCODE.
    $result = @{ type = 'result'; success = $true; url = 'http://127.0.0.1:18080/' }
    [Console]::Out.WriteLine(($result | ConvertTo-Json -Compress))
    exit 0
} catch {
    $result = @{ type = 'result'; success = $false; error = $_.Exception.Message }
    [Console]::Out.WriteLine(($result | ConvertTo-Json -Compress))
    exit 1
}
```

持续 Node.js 本地服务（配置 `runner: "custom"`、解释器 `node.exe`、`mode: "persistent"`）：

```js
import { createServer } from 'node:http'
const server = createServer((_request, response) => response.end('Ready'))
server.on('error', error => { console.error(error.message); process.exit(1) })
server.listen(0, '127.0.0.1', () => {
  console.log(JSON.stringify({
    type: 'result', success: true,
    url: `http://127.0.0.1:${server.address().port}/`,
  }))
})
```

## 宿主接口与验证

接口只供本地 `main` WebView 使用，均验证调用者身份与来源。外部网页和弹窗不获得配置、文件选择、执行、日志或停止权限。

| 接口 | 用途 |
| --- | --- |
| `workspace_save_entry` | 新增可选 `preparation`；`replacePreparation: true` 明确替换或清除，旧调用省略时保留既有绑定 |
| `preparation_pick_file` | 在主窗口拥有的原生文件选择器中选择程序，取消返回空值 |
| `preparation_states` | 查询已运行程序状态、运行代次、退出码及配置变更提示 |
| `preparation_logs` | 按 `entryId` 读取当前代次的有界日志 |
| `preparation_stop` | 显式停止对应托管程序 |
| `web_prepare` | 首次准备或 `retry: true` 重新准备并加载 |
| `web-prepared` | 只向主界面通知成功的入口 ID，使网页清除旧失败等待并显示新结果 |

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p oriel-core --locked
cargo clippy --manifest-path src-tauri/Cargo.toml -p oriel-core --locked --all-targets -- -D warnings
npm run build
```

核心执行测试使用已有 Node.js，可用 `ORIEL_TEST_NODE` 指定解释器，不安装依赖。Unix 测试进程组仅验证调度与取消，不作为 Windows Job Object 验证依据。

Windows 构建前端及调试程序后运行 `npm run check:preparation`。数据、脚本与截图写入新的 `target/preparation-check-*`，使用本机测试站点及隔离子进程，不读取个人脚本或 SSH 配置；另运行既有浏览器、主题与隔离 Tether 回归。文件选择器实际操作及截图视觉审查需要人工复核。

本轮已通过 43 项核心测试、核心 Clippy（所有目标且警告视为错误）、改动 Rust 文件的格式检查，以及前端类型检查与生产构建。工作区依赖缺少 Linux Rolldown／Lightning CSS 绑定，前端最终在临时隔离目录复用与当前 lockfile 一致的既有依赖构建通过，未修改依赖版本或 lockfile。Windows 验收脚本及夹具已通过 JavaScript 语法检查；正式文档和 skill 的 68 处本地链接有效，skill 校验器因缺少 PyYAML 未运行成功，已人工检查 frontmatter。

宿主整机检查因 Linux 缺少 `pkg-config` 停在系统依赖阶段；WSL 调用 Windows 程序报 `UtilBindVsockAnyPort: socket failed`，且当前 Rust 只有 Linux targets。未安装全局工具或系统库；Windows 宿主编译、进程树回收、原生交互与截图审查尚未执行，历史结果不作为本轮通过依据。
