# 本地服务与前置资源 v1

2026-10-21：已实现 Windows 最小生命周期与前置转发接入。现有版本 1 网页声明保持兼容；引入时工作区配置为版本 2；当前配置版本及迁移见[网页前置程序](web-page-preparation.md)，此处旧接入契约保持不变。此能力没有安装、更新、通用工作流或远程 shell 执行接口。

## 声明

本地用户登记受信任目录，显式打开服务页面或依赖页面时才运行生命周期入口。读取声明、重载配置不运行入口。外部网页不能登记目录、发起宿主前置动作或取得宿主命令权限。

```json
{
  "schemaVersion": 2,
  "id": "tether",
  "name": "Tether",
  "version": "0.1.0",
  "capabilities": ["web-page", "local-service-v1"],
  "lifecycle": {"protocol":"stdio-v1","entrypoint":"oriel.ps1","args":[]},
  "entries": [{"id":"workspace","title":"Tether","kind":"web-page","service":"tether","proxy":{"mode":"direct"}}]
}
```

生命周期入口须为登记目录内存在的 `.ps1` 文件，不允许绝对路径、上级路径或指向目录外的链接。参数使用结构化列表。Windows 通过 PowerShell 的进程级执行策略运行，既不修改全局策略，也不自动安装依赖。

服务页面的实际地址由后台就绪结果返回，不把开发服务器地址写进发布声明。纯页面不需要生命周期；可在版本 2 声明中引用前置资源：

```json
{
  "schemaVersion": 2,
  "id": "operations",
  "name": "运维",
  "version": "1",
  "capabilities": ["web-page"],
  "entries": [{
    "id":"web", "title":"管理页面", "kind":"web-page",
    "url":"http://127.0.0.1:18080", "proxy":{"mode":"direct"},
    "prerequisites":[{"pluginId":"tether","ruleId":"ops-web"}]
  }]
}
```

用户固定页签也可在本地 `workspace.json` 的对应 `webEntries` 项添加相同 `prerequisites`。当前界面尚无依赖编辑表单；编辑标题或地址会保留已配置的引用。SSH 参数与密钥只在 Tether 中维护。

## 生命周期与就绪

`stdio-v1` 入口前台运行，保留 stdin/stdout。首行报告 `{protocol:1, ready:true, instance, origin, token, pageUrl}`；宿主检查版本、实例身份与回环页面地址，再用 status 往返核验。随后每行一个公共动作请求和一个 `{ok,value/error}` 响应。启动入口退出码或任意端口开放均不表示业务就绪。

宿主按稳定规则 ID 调用 `ensure`，核对同一实例和 `ready:true` 后才创建依赖页面。后台的规则结果可能为复用、运行、参数待应用、冲突或失败；只有确认运行满足前置依赖。

等待不持有网页展示锁，可打开 Tether 页面处理认证；错误保留诊断并由显式“重试”再次请求。已有页面切换、调宽、隐藏、关闭或加载失败都不回收转发。用户在 Tether 手动停止后，旧页面和状态轮询不强制重新启动。

## 退出和权限

完整退出时先关闭依赖网页视图，再通过已验证实例的受认证回环接口请求 shutdown，取消待认证、待重连及托管转发，最后结束包装进程。stdin EOF 同样请求清理；不依据历史 PID、端口或进程名批量结束 SSH。

接入已有同配置实例会复用它；退出清理包括该实例内手动、临时、计划和前置动作启动的全部托管转发。终端及其他程序启动的外部 SSH 和 Agent 不动。多 Oriel 会话共享实例的仲裁尚未实现。

页面依旧位于独立原生 WebView，不能调用宿主命令或访问宿主文件。Tether 页面只用自己的随机回环地址与身份调用业务后台；外部网页不会因设置前置资源获得这些凭证。协议首行的 token 不进入前端工作区快照。

## 本轮验证

父仓库运行核心测试、前端构建和 Windows 宿主构建。`scripts/check-tether.mjs` 在隔离目录中使用子仓库已安装的 SSH2 测试依赖与 Windows Debug 程序，覆盖真实前置就绪、双页面复用、停止后切换、失败诊断、等待时打开认证页面、外部网页权限和认证等待中完整退出取消请求并保留外部 SSH。脚本可用 `TETHER_REPO` 指定独立工具位置，不形成生产运行时源码依赖。

```powershell
node scripts/check-tether.mjs
```

未覆盖系统挂起/重启、所有客户端版本和多宿主仲裁。缺入口、后台退出或超时会诊断，超时回收只基于已创建的包装进程；不通过全局进程清理掩盖失败。

本轮最终结果：22 项核心测试、8 项 Tether 联调及既有 30 项隔离浏览器回归通过；原生密码保存提示仍是已有人工验收项，未将其记为新增通过。

最后一轮认证中退出后出现一次 WebView PostMessage 无效窗口句柄诊断，退出及资源归属断言仍通过；该退出回调日志尚未收敛。
