# 网页前置程序

2026-10-24：新增按网页独立执行的本地前置程序，保留原有[本地服务与规则引用](local-services.md)。本文是本次配置和输出约定，不恢复已撤回的 script-events-v1，也不建立通用脚本插件或资源依赖图。Windows 为产品目标平台；本轮实际验证见文末。

## 使用与职责

在网页设置中启用“前置程序”，选择本地程序或脚本；保存、目录发现和重新加载接入不执行程序。首次打开关联网页，或明确选择“重新准备并加载”时执行。没有前置程序的网页保持原行为。

Oriel 管理执行实例、输出、准备结果和实际拥有的进程树；脚本封装具体工具差异。用户可以让脚本调用其他插件或独立工具的公开接口，缺少工具、解释器或接口时正常报错。工具负责自己的连接或服务管理，界面操作和脚本调用可以复用同一业务能力。脚本取得调用结果不自动取得外部工具资源的停止权。

每个网页独立执行。两个网页调用同一程序也不由宿主合并；被调用工具仍可自行复用资源。后台显示的是进程与执行结果，不持续检查端口、服务业务健康或自动重连。

## 管理入口

2026-10-26：全局“前置程序”入口位于工作台设置，一级侧栏不再常驻三点按钮。在首页或其他插件中也可进入，选择关联网页，查看执行状态、旧配置对应的当前运行记录与日志，显式停止或重新准备。配置不可写不阻止进入管理面板；没有关联配置或运行记录时显示添加配置的提示。

浏览器工具栏仅在当前网页有前置程序时显示终端图标，可访问名称与悬停提示均为“当前网页的前置程序”，打开同一面板并预选该网页。面板内仍允许切换其他关联网页。从工作台设置打开时暂时关闭设置弹窗，保留表单；关闭管理面板或按 Escape 后重新显示原设置并恢复入口焦点。整个过程持续隐藏原生网页视图，不保存设置、不启动或停止程序。工具应用中的前置程序入口继续放在应用操作菜单中。

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

版本 1／2 插件仍保持既有语义。旧 `service`、`prerequisites`、`lifecycle` 不自动转换或删除；同一入口不能同时配置新前置程序与旧服务／规则引用；本次失败后继续访问的规则仅作用于新 `preparation`，旧服务／规则依赖保持原行为。浏览器工作区入口自身不能声明前置程序，各网页分别配置。

工作区版本 1／2／3 只在内存中迁移，首次成功保存才写为 4，保留稳定 ID、网页顺序、代理、主题、插件目录和浏览数据。异常配置保留原文件并阻止覆盖。旧版 Oriel 不能读取版本 4，避免忽略前置步骤直接加载网页。

## 程序启动上下文

Oriel 为每次前置程序设置环境变量 `ORIEL_PAGE_URL` 与 `ORIEL_RESULT_FILE`：
- `ORIEL_PAGE_URL`：该网页当前登记的规范化地址；没有登记地址时显式设为空，避免继承其他运行的地址。改变登记地址会使旧准备结果过期，需要显式重新准备后传入新上下文，但不妨碍直接访问新的登记地址。
- `ORIEL_RESULT_FILE`：宿主为本次运行生成的唯一临时通信文件路径。新脚本可直接将就绪结果 JSON 写入此文件，享受免受外部命令 stdout 杂质日志干扰的双轨通信。

SSH 模板的 `PageUrl` 留空时读取此变量，仅用它判断本地监听端口，成功结果仍省略 `url`，由宿主使用登记地址。独立运行且没有此变量时须显式填写 `PageUrl`。

## 输出与双轨通信 (Dual-Track IPC)

宿主并发支持结果文件与旧 stdout 两个通道。未选定结果时，每轮先检查文件，再消费 stdout；选定后不切换通道。文件选定后 stdout 仅作日志；stdout 选定后仍检查重复结果，不再被迟到文件替换。

- 文件结果须为不超过 64 KiB 的 UTF-8 JSON。运行中允许尚未写完的 JSON 继续等待，退出后仍不完整时报错；空文件不构成结果。建议脚本完成写入后再退出或报告就绪。
- stdout 只将完整、以换行结束且不超过 64 KiB 的行交给协议判定。非协议文本先记录为日志；没有有效结果且程序结束时诊断。旧脚本可继续输出约定 JSON 行，无需改为文件。
- stdout 读取与协议判定分离，长日志和未换行尾段按最多 4 KiB 分段记录，不因格式错误终止读取线程。EOF、读取错误与协议错误分别处理，读取结束或通道断开均结束等待；文件通道成功不依赖 stdout 的协议格式或 EOF。
- stdout 和 stderr 进入每次运行最多 256 条、每条最多 4 KiB 的内存日志缓冲。失败、超时、取消及正常结束均清理所属进程树与临时文件；持续程序就绪后保留文件至停止或退出清理。

结果格式约定保持不变：
UTF-8、不超过 64 KiB 的 JSON 结果：

```json
{"type":"result","success":true,"url":"http://127.0.0.1:18080/"}
```

```json
{"type":"result","success":false,"error":"具体失败原因"}
```

成功可以省略 `url`，此时使用网页登记地址。前置程序启动失败、返回失败、格式错误或超时等情况，有有效登记地址时仍尝试打开该网页，失败状态、原因和日志继续保留在后台，不将程序标记为成功。没有登记地址且未取得有效返回地址时才无法继续打开。地址沿用 HTTP／HTTPS 校验，拒绝宿主内部地址、含账号密码的 URL 和其他协议。返回地址只用于本次运行，不覆盖登记地址、入口 ID 或浏览数据目录。协议结果拒绝未知字段；stdout 通道的重复结果失败，非 JSON、无效 UTF-8 或未换行输出在未得到有效结果时诊断，文件通道选定后这些 stdout 内容仅为日志。

一次性任务必须返回成功结果，并以退出码 0 结束，才算准备完成。文件成功且入口退出 0 时直接完成，随后清理所属进程树，不等待后代持有的 stdout 管道关闭。只输出成功却不退出仍受准备超时限制。任务结束后回收其残留子进程；不能用一次性任务偷偷遗留由宿主启动的服务器。

持续程序返回成功时允许加载网页，入口继续前台运行，后续不输出第二条准备结果。该入口退出（包括退出码 0）记为持续程序失败，保留已经打开的网页，不自动重启。持续服务应由入口维持，或调用独立工具的公开接口后以一次性任务结束，由工具管理外部资源。

结果输出失败、启动失败、异常退出、超时和日志分别可见。每次运行最多保留 256 条日志，每条最多 4 KiB，结果与日志不持久化为用户配置。

## 生命周期与网页

同页并发准备合并为一个任务；不同页不共用执行实例。准备等待不占用网页展示锁，也不阻止切换到其他页面。每次运行带独立代次，旧配置或旧运行的结果不得导航当前页面。

| 操作 | 行为 |
| --- | --- |
| 首次打开 | 优先使用成功返回的地址，否则使用登记地址；程序失败也尝试登记网页 |
| 切换、隐藏、调宽及普通刷新 | 不重复执行、不停止程序；刷新只作用于网页 |
| 重新准备并加载 | 清理该页旧托管程序，重新执行；相同地址也刷新已有网页 |
| 编辑、保存或重新加载配置 | 不执行新程序、不自动替换旧程序；旧结果失效后仍可访问登记地址，显式重新准备才执行新命令 |
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

### SSH 本地端口转发模板

可直接复制 [ssh-port-forward.ps1](../../plugins/builtin/browser/script/ssh-port-forward.ps1) 多份，在每个文件顶部粘贴完整 SSH 命令。`PageUrl` 默认留空，使用 Oriel 登记地址来检查本地端口，成功结果不再返回 `url`；需要覆盖地址或独立运行脚本时才填写它。登记地址或显式地址的端口应与 `-L` 本地端口一致。Oriel 选择该文件，执行方式设为自动或 PowerShell，运行模式设为持续程序，准备超时建议 120 秒。保留 UTF-8 BOM 编码。

模板直接启动 SSH，使用 [Get-NetTCPConnection 的 OwningProcess](https://learn.microsoft.com/en-us/powershell/module/nettcpip/get-nettcpconnection) 确认本次 SSH 拥有目标本地监听后，只输出一条成功结果，并保持脚本运行。已有监听时跳过本次 SSH 启动，避免等待认证或连接；不将占用视为本次 SSH 成功，也不接管其他进程。占用或其他失败保留诊断，Oriel 仍尝试登记网页，允许已有外部转发实际提供访问。SSH 日志写标准错误。密钥／Agent 和主机指纹需事先准备；模板不交互询问密码，不通过 Tether 接管其他连接。监听就绪不保证远端服务可用，[ExitOnForwardFailure](https://man.openbsd.org/ssh_config#ExitOnForwardFailure) 也不保证转发目标的业务连接成功。

本模板尚未运行真实 SSH；当前 WSL 无法启动 Windows PowerShell，原生语法解析与隔离联调待可用 Windows 环境执行。

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

## 2026-10-24：SSH 模板与可选准备

新增可复制的完整 SSH 命令模板，并将 `PageUrl` 调整为默认留空。宿主传递登记地址上下文，程序失败不阻断有效登记地址的访问，仍保留程序失败和进程清理边界；运行代次检查也覆盖失败结果，过期失败不能覆盖新的页面结果。

本次通过 46 项核心测试和核心 Clippy，覆盖省略地址、失败回退、实际程序失败状态、登记地址上下文及变更；Windows 验收脚本扩展了模板无地址和外部端口占用场景，使用隔离的模拟 SSH 程序，不连接真实 SSH。脚本语法检查通过，Windows 原生编译、PowerShell 解析和联调仍因 WSL 互操作限制待运行；没有执行真实 SSH。

## 2026-10-26：前置程序双轨通信升级

新增 `ORIEL_RESULT_FILE` 文件通道，保留旧 stdout JSON 行；两者与有界日志缓冲配合，避免将文件通道的 stdout 调试输出判作结果失败。当前仲裁和清理约定以上文为准，不将历史兼容或测试数量视为本轮结果。

## 2026-10-27：优化批次验证

本批保留悬浮层、插件插槽、双通道通信和按需图标读取，沿用依赖锁定版本、配置版本及旧插件默认行为。回归覆盖文件结果配合无换行尾段、超过 64 KiB 的日志、持续程序继续输出、后代持有 stdout 时仍完成文件结果、旧 stdout、失败、取消、超时、重复结果和临时文件清理。前端状态检查覆盖图标身份／地址失效、迟到请求、更新事件优先级和删除重建，以及悬浮层请求互斥、焦点恢复、初始化／呈现超时和再次打开。

验证命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p oriel-core --locked
cargo clippy --manifest-path src-tauri/Cargo.toml -p oriel-core --locked --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
npm run check:ui-state
npm run build
git diff --check
```

WSL 按 [隔离构建](../../README.md#wsl-隔离构建)复制当前源码和 lockfile、运行 `npm ci --offline --include=optional`，状态检查另复制 `scripts/`；不覆盖共享工作区依赖。本轮 55 项核心测试、核心 Clippy（所有 target、警告视为错误）、格式检查、前端状态检查及 WSL 隔离生产构建通过；相关 Node 脚本语法检查与 `git diff --check` 通过。

完整 Linux 宿主 `cargo check --locked` 停在 `libdbus-sys` 构建阶段：环境没有 `pkg-config`，无法查询 `dbus-1`；未安装系统依赖。当前 Rust 仅安装 Linux GNU／musl target，没有 Windows target，因此 Windows 宿主编译和原生交互待运行。建议在具备原有 Windows 工具链的工作站执行宿主构建及 `npm run check:browser`、`npm run check:preparation`、`npm run check:theme`。这些脚本已区分 main／overlay 调试目标，补充当前网页预选、Escape 返回设置、草稿和焦点恢复、原生关闭再打开、保存期间关闭受阻、主题双向同步及两种布局的插槽动作；本批仅做脚本语法检查，未宣称原生验收通过。不同 DPI、移动缩放、最小化恢复和悬浮视觉效果仍需人工验收。
