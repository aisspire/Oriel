#requires -Version 5.1
# Oriel 前置程序：执行方式选“自动”或“PowerShell”，运行模式选“持续程序”。
# 可以复制本文件多份；修改完整 SSH 命令即可，PageUrl 默认留空使用 Oriel 登记地址。

# ==================== 完整 SSH 命令：直接在这里粘贴 ====================
$SshCommand = @'
ssh.exe -N -T -o BatchMode=yes -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes -o ForkAfterAuthentication=no -o ConnectTimeout=15 -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -L 127.0.0.1:18080:127.0.0.1:8080 -p 22 user@example.com
'@
# ======================================================================

# 可留空：读取 Oriel 登记地址来检查端口，成功结果不返回 url，打开登记网页。
# 需要覆盖登记地址或独立运行时再填，例如 'http://127.0.0.1:18080/admin/'。
# 用于检查的网页地址端口须与上面 -L 的本地端口一致。
$PageUrl = ''
$ReadyTimeoutSeconds = 90 # Oriel 准备超时建议设为 120 秒，略大于此值。

# 命令示例（均为一整行，替换 user@example.com、端口和路径）：
# ssh.exe -N -T -o BatchMode=yes -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes -o ForkAfterAuthentication=no -L 127.0.0.1:18080:127.0.0.1:8080 -p 22 user@example.com
# ssh.exe -N -T -o BatchMode=yes -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes -o ForkAfterAuthentication=no -i "C:\Users\you\.ssh\id_ed25519" -L 127.0.0.1:18080:10.0.0.20:8080 -p 2222 user@example.com
#
# 只粘贴单条 SSH 命令；带空格的路径使用双引号，不添加 &、start 或 -f。
# 保留 -N、BatchMode=yes、ExitOnForwardFailure=yes、ForkAfterAuthentication=no。
# 使用 SSH 配置别名时，将 user@example.com 换成 Host 别名；沿用别名的 Port 时移除 -p 22。
# 在终端预先确认主机指纹，并准备密钥或 SSH Agent；此脚本不交互询问密码。
# 就绪仅代表本次 SSH 已建立对应的本地监听，不代表远端网页服务健康。
# 以下包装部分通常不需要修改。不接管已有 SSH，也不将其他进程的监听报告为本次成功。
# SSH 失败（例如端口已占用）会保留失败诊断；Oriel 仍会尝试打开登记网页。

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$script:PreparationResultWritten = $false
$sshProcess = $null
$scriptExitCode = 1

function Write-PreparationResult {
    param([bool]$Success, [string]$Message)
    if ($script:PreparationResultWritten) { return }
    $result = @{ type = 'result'; success = $Success }
    if ($Success) {
        if (-not [string]::IsNullOrWhiteSpace($PageUrl)) { $result.url = $PageUrl }
    } else { $result.error = $Message }
    [Console]::Out.WriteLine(($result | ConvertTo-Json -Compress))
    [Console]::Out.Flush()
    $script:PreparationResultWritten = $true
}

function Receive-SshOutput {
    param([ref]$PendingLine, [System.IO.StreamReader]$Reader)
    # 两条输出流都消费；每轮限量读取，避免日志阻塞准备和退出检查。
    for ($lineIndex = 0; $lineIndex -lt 32; $lineIndex++) {
        if ($null -eq $PendingLine.Value -or -not $PendingLine.Value.IsCompleted) { return }
        $line = $PendingLine.Value.GetAwaiter().GetResult()
        if ($null -eq $line) { $PendingLine.Value = $null; return }
        [Console]::Error.WriteLine($line)
        $PendingLine.Value = $Reader.ReadLineAsync()
    }
}

try {
    if ($env:OS -ne 'Windows_NT') { throw '此示例用于 Windows PowerShell 和 Windows OpenSSH。' }
    if ($ReadyTimeoutSeconds -lt 1 -or $ReadyTimeoutSeconds -gt 3600) { throw '准备超时须为 1–3600 秒。' }

    $effectivePageUrl = $PageUrl
    if ([string]::IsNullOrWhiteSpace($effectivePageUrl)) { $effectivePageUrl = $env:ORIEL_PAGE_URL }
    if ([string]::IsNullOrWhiteSpace($effectivePageUrl)) {
        throw 'PageUrl 留空时需要在 Oriel 中登记网页地址；独立运行此脚本时请填写 PageUrl。'
    }
    $pageUri = $null
    if (-not [Uri]::TryCreate($effectivePageUrl, [UriKind]::Absolute, [ref]$pageUri) -or
        $pageUri.Scheme -notin @('http', 'https') -or -not $pageUri.IsLoopback -or
        -not [string]::IsNullOrEmpty($pageUri.UserInfo)) {
        throw '用于检查的网页地址须为不含账号密码、指向本机回环地址的 HTTP/HTTPS 网页。'
    }
    $forwardHost = $pageUri.DnsSafeHost.Trim([char[]]'[]')
    if ($forwardHost -eq 'localhost') { $listenAddresses = @('127.0.0.1', '::1', '0.0.0.0', '::') }
    elseif ($forwardHost.Contains(':')) { $listenAddresses = @($forwardHost, '::') }
    else { $listenAddresses = @($forwardHost, '0.0.0.0') }

    $commandLine = $SshCommand.Trim()
    if ($commandLine -match '[\r\n]') { throw '请将完整 SSH 命令粘贴为一行。' }
    $commandMatch = [regex]::Match($commandLine, '^(?:"(?<program>[^"]+)"|(?<program>[^\s"]+))\s+(?<arguments>.+)$')
    if (-not $commandMatch.Success) { throw 'SSH 命令格式无效，应以 ssh.exe 或其双引号包围的完整路径开头。' }
    $sshExecutable = $commandMatch.Groups['program'].Value
    if ([IO.Path]::GetFileName($sshExecutable) -notin @('ssh', 'ssh.exe')) { throw '此模板只执行 ssh/ssh.exe 命令。' }
    $sshApplication = Get-Command -Name $sshExecutable -CommandType Application -ErrorAction Stop | Select-Object -First 1
    $null = Get-Command -Name Get-NetTCPConnection -ErrorAction Stop

    # 已有监听时不再尝试认证/连接 SSH，也不接管该进程。
    # 保留“本次准备失败”的诊断，由 Oriel 照常尝试打开登记网页。
    $existingProbeErrors = @()
    $existingListeners = @(Get-NetTCPConnection -LocalPort $pageUri.Port -State Listen -ErrorAction SilentlyContinue -ErrorVariable existingProbeErrors)
    foreach ($probeError in $existingProbeErrors) {
        if ($probeError.CategoryInfo.Category -ne [System.Management.Automation.ErrorCategory]::ObjectNotFound) {
            throw "无法检查本地监听：$probeError"
        }
    }
    foreach ($listener in $existingListeners) {
        if ($listener.LocalAddress -in $listenAddresses) {
            throw "本地端口 $($pageUri.Port) 已有监听（PID $($listener.OwningProcess)），跳过本次 SSH 启动；Oriel 仍将尝试登记网页。"
        }
    }

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $sshApplication.Source
    # 直接传递命令中的 SSH 参数，不经过 CMD/Invoke-Expression，不执行其他 Shell 命令。
    $startInfo.Arguments = $commandMatch.Groups['arguments'].Value
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardInput = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $sshProcess = [System.Diagnostics.Process]::new()
    $sshProcess.StartInfo = $startInfo
    if (-not $sshProcess.Start()) { throw '无法启动 SSH。' }
    $sshProcess.StandardInput.Close()
    $stdoutLine = $sshProcess.StandardOutput.ReadLineAsync()
    $stderrLine = $sshProcess.StandardError.ReadLineAsync()
    $startupTimer = [System.Diagnostics.Stopwatch]::StartNew()
    $nextProbeMilliseconds = 0
    [Console]::Error.WriteLine("SSH 已启动，等待本次进程监听 $($pageUri.Host):$($pageUri.Port)。")

    while ($true) {
        Receive-SshOutput -PendingLine ([ref]$stdoutLine) -Reader $sshProcess.StandardOutput
        Receive-SshOutput -PendingLine ([ref]$stderrLine) -Reader $sshProcess.StandardError
        if ($sshProcess.HasExited) {
            Receive-SshOutput -PendingLine ([ref]$stdoutLine) -Reader $sshProcess.StandardOutput
            Receive-SshOutput -PendingLine ([ref]$stderrLine) -Reader $sshProcess.StandardError
            throw "SSH 已退出，退出码 $($sshProcess.ExitCode)。请查看上方 SSH 日志。"
        }
        if (-not $script:PreparationResultWritten) {
            if ($startupTimer.Elapsed.TotalSeconds -ge $ReadyTimeoutSeconds) {
                throw "等待 SSH 本地端口超时。请确认命令的 -L 本地端口与登记地址或 PageUrl 一致：$effectivePageUrl"
            }
            if ($startupTimer.ElapsedMilliseconds -ge $nextProbeMilliseconds) {
                $nextProbeMilliseconds = $startupTimer.ElapsedMilliseconds + 500
                $probeErrors = @()
                $listeners = @(Get-NetTCPConnection -OwningProcess $sshProcess.Id -LocalPort $pageUri.Port -State Listen -ErrorAction SilentlyContinue -ErrorVariable probeErrors)
                foreach ($probeError in $probeErrors) {
                    if ($probeError.CategoryInfo.Category -ne [System.Management.Automation.ErrorCategory]::ObjectNotFound) {
                        throw "无法确认本次 SSH 的端口归属：$probeError"
                    }
                }
                foreach ($listener in $listeners) {
                    if ($listener.OwningProcess -eq $sshProcess.Id -and $listener.LocalAddress -in $listenAddresses -and -not $sshProcess.HasExited) {
                        Write-PreparationResult -Success $true
                        [Console]::Error.WriteLine('SSH 本地监听已就绪；脚本将保持运行，退出或停止由 Oriel 管理。')
                        break
                    }
                }
            }
        }
        Start-Sleep -Milliseconds 100
    }
} catch {
    $message = $_.Exception.Message
    [Console]::Error.WriteLine($message)
    # 就绪后故障只写日志并退出，不再发送第二条结果。
    Write-PreparationResult -Success $false -Message $message
} finally {
    if ($null -ne $sshProcess) {
        try {
            if (-not $sshProcess.HasExited) {
                $sshProcess.Kill()
                if (-not $sshProcess.WaitForExit(5000)) {
                    [Console]::Error.WriteLine('未能及时确认本次 SSH 退出，由 Oriel 继续回收所属进程树。')
                }
            }
        } catch { [Console]::Error.WriteLine("本次 SSH 清理诊断：$($_.Exception.Message)") }
        $sshProcess.Dispose()
    }
}
exit $scriptExitCode
