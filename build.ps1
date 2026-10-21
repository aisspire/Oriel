# Windows packaging entrypoint. No global installation or system configuration changes.
[CmdletBinding()]
param(
    [switch]$NoBundle,
    [switch]$InstallDependencies,
    [switch]$Check,
    [string]$TargetDir = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$buildExitCode = 0
$locationPushed = $false
$hadTargetDirectory = Test-Path Env:\CARGO_TARGET_DIR
$previousTargetDirectory = $env:CARGO_TARGET_DIR

function Get-BuildTool([string]$Name) {
    $tool = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $tool) {
        throw "Required tool '$Name' is missing. Install the Windows Tauri prerequisites, then run this script again."
    }
    return $tool.Source
}

function Invoke-BuildTool([string]$Executable, [string[]]$ArgumentList) {
    & $Executable @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        $script:buildExitCode = $LASTEXITCODE
        throw "Command failed (exit code ${LASTEXITCODE}): $Executable $($ArgumentList -join ' ')"
    }
}

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw 'Run build.cmd or build.ps1 from Windows, using Windows Node.js and Rust.'
    }
    if ($Check -and $InstallDependencies) {
        throw '-Check does not install dependencies. Run -InstallDependencies separately.'
    }
    Push-Location -LiteralPath $PSScriptRoot
    $locationPushed = $true

    $nodeExecutable = Get-BuildTool 'node.exe'
    $npmExecutable = Get-BuildTool 'npm.cmd'
    $null = Get-BuildTool 'cargo.exe'
    $null = Get-BuildTool 'rustc.exe'
    foreach ($lockFile in @('package-lock.json', 'src-tauri/Cargo.lock')) {
        if (-not (Test-Path -LiteralPath $lockFile -PathType Leaf)) {
            throw "Required lockfile is missing: $lockFile"
        }
    }
    $package = Get-Content -LiteralPath 'package.json' -Raw -Encoding UTF8 | ConvertFrom-Json
    $nodeVersion = & $nodeExecutable -p 'process.versions.node'
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the Node.js version.' }
    if ($package.engines.node -notmatch '^>=(\d+\.\d+\.\d+)$') {
        throw 'The Node.js engine constraint changed; update the version check in build.ps1.'
    }
    if ([version]$nodeVersion -lt [version]$Matches[1]) {
        throw "Node.js $($package.engines.node) is required; found $nodeVersion."
    }
    if ([string]::IsNullOrWhiteSpace($TargetDir)) {
        $TargetDir = if ($hadTargetDirectory -and -not [string]::IsNullOrWhiteSpace($previousTargetDirectory)) {
            $previousTargetDirectory
        } else { Join-Path $PSScriptRoot 'target/windows' }
    }
    if (-not [IO.Path]::IsPathRooted($TargetDir)) { $TargetDir = Join-Path $PSScriptRoot $TargetDir }
    $TargetDir = [IO.Path]::GetFullPath($TargetDir)

    if ($InstallDependencies) {
        Invoke-BuildTool $npmExecutable @('ci', '--no-audit', '--no-fund', '--cache', (Join-Path $PSScriptRoot 'target/npm-cache'))
    }
    $cliScript = Join-Path $PSScriptRoot 'node_modules/@tauri-apps/cli/tauri.js'
    if (-not (Test-Path -LiteralPath $cliScript -PathType Leaf)) {
        throw 'Project dependencies are missing. Run build.cmd -InstallDependencies (or npm ci) using Windows Node.js.'
    }
    # Loading the local CLI also verifies that its Windows native binding is usable.
    Invoke-BuildTool $nodeExecutable @($cliScript, '--version')
    Write-Host "Node.js: $nodeVersion"
    Write-Host "Build directory: $TargetDir"
    if ($Check) {
        Write-Host 'Environment check passed. No build or installation was performed.'
    } else {
        $env:CARGO_TARGET_DIR = $TargetDir
        $buildArguments = @('run', 'tauri', '--', 'build', '--ci')
        if ($NoBundle) { $buildArguments += '--no-bundle' }
        else { $buildArguments += @('--bundles', 'nsis') }
        $buildArguments += @('--', '--locked')
        Invoke-BuildTool $npmExecutable $buildArguments

        $releaseDirectory = $TargetDir
        if (-not [string]::IsNullOrWhiteSpace($env:CARGO_BUILD_TARGET)) {
            $releaseDirectory = Join-Path $releaseDirectory $env:CARGO_BUILD_TARGET
        }
        $releaseDirectory = Join-Path $releaseDirectory 'release'
        $executablePath = Join-Path $releaseDirectory 'oriel.exe'
        if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
            throw "Build finished but the executable was not found at $executablePath. Check your Cargo target configuration."
        }
        Write-Host "Executable: $executablePath"
        if (-not $NoBundle) {
            $installerDirectory = Join-Path $releaseDirectory 'bundle/nsis'
            $installers = @(Get-ChildItem -LiteralPath $installerDirectory -Filter '*-setup.exe' -File)
            if ($installers.Count -eq 0) { throw "No NSIS installer was generated in $installerDirectory." }
            foreach ($installer in $installers) { Write-Host "Installer: $($installer.FullName)" }
        }
        Write-Host 'Build completed successfully.'
    }
} catch {
    if ($buildExitCode -eq 0) { $buildExitCode = 1 }
    [Console]::Error.WriteLine("Build failed: $($_.Exception.Message)")
} finally {
    if ($hadTargetDirectory) { $env:CARGO_TARGET_DIR = $previousTargetDirectory }
    else { Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
    if ($locationPushed) { Pop-Location }
}
exit $buildExitCode
