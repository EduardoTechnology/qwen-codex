param(
    [string]$BaseUrl = $env:QWEN_CODEX_BASE_URL,
    [string]$Model = $env:QWEN_CODEX_MODEL,
    [string]$ApiKey = $env:QWEN_CODEX_API_KEY,
    [string]$ContextWindow = $env:QWEN_CODEX_CONTEXT_WINDOW,
    [switch]$Release,
    [string]$InstallDir = (Join-Path $env:USERPROFILE ".cargo\bin")
)

if ([string]::IsNullOrWhiteSpace($BaseUrl)) {
    $BaseUrl = "http://127.0.0.1:8002/v1"
}
if ([string]::IsNullOrWhiteSpace($ApiKey)) {
    $ApiKey = "local-dev-key"
}

$BaseUrl = $BaseUrl.TrimEnd("/")
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$ProfileDir = if ($Release) { "release" } else { "debug" }
$CargoArgs = @("build", "-p", "codex-cli", "--bins")
if ($Release) {
    $CargoArgs += "--release"
}

function Initialize-QwenMetadata {
    if (-not [string]::IsNullOrWhiteSpace($Model) -and -not [string]::IsNullOrWhiteSpace($ContextWindow)) {
        return
    }
    try {
        $models = Invoke-RestMethod -Uri "$BaseUrl/models" -Method Get -TimeoutSec 10 -ErrorAction Stop
        if ([string]::IsNullOrWhiteSpace($Model) -and $models.data -and $models.data.Count -gt 0 -and $models.data[0].id) {
            Set-Variable -Name Model -Scope Script -Value $models.data[0].id
        }
        if ([string]::IsNullOrWhiteSpace($ContextWindow) -and $models.data -and $models.data.Count -gt 0 -and $models.data[0].max_model_len) {
            Set-Variable -Name ContextWindow -Scope Script -Value ([string]$models.data[0].max_model_len)
        }
    } catch {
        # Fall through to documented local defaults.
    }
    if ([string]::IsNullOrWhiteSpace($Model)) {
        Set-Variable -Name Model -Scope Script -Value "qwen35-local"
    }
    if ([string]::IsNullOrWhiteSpace($ContextWindow)) {
        Set-Variable -Name ContextWindow -Scope Script -Value "32768"
    }
}

function Write-QwenCmdWrapper {
    param(
        [string]$Name,
        [string]$RealExe
    )

    $WrapperPath = Join-Path $InstallDir "$Name.cmd"
    Remove-Item -Force -ErrorAction SilentlyContinue $WrapperPath
    $Content = @"
@echo off
if not defined QWEN_CODEX_BASE_URL set "QWEN_CODEX_BASE_URL=$BaseUrl"
if not defined QWEN_CODEX_MODEL set "QWEN_CODEX_MODEL=$Model"
if not defined QWEN_CODEX_API_KEY set "QWEN_CODEX_API_KEY=$ApiKey"
if not defined QWEN_CODEX_CONTEXT_WINDOW set "QWEN_CODEX_CONTEXT_WINDOW=$ContextWindow"
"%~dp0$RealExe" %*
"@
    Set-Content -Path $WrapperPath -Value $Content -Encoding ASCII
}

function Install-Binary {
    param(
        [string]$Source,
        [string]$Destination
    )

    Remove-Item -Force -ErrorAction SilentlyContinue $Destination
    Copy-Item -Path $Source -Destination $Destination
}

Initialize-QwenMetadata

Write-Host "Building qwen-codex from local source..."
Push-Location (Join-Path $RepoRoot "codex-rs")
try {
    cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$TargetDir = Join-Path (Join-Path $RepoRoot "codex-rs\target") $ProfileDir
foreach ($Bin in @("codex", "qwen-codex", "qwencodex")) {
    $Source = Join-Path $TargetDir "$Bin.exe"
    if (-not (Test-Path $Source)) {
        Write-Error "expected binary not found: $Source"
        exit 1
    }
}
Install-Binary -Source (Join-Path $TargetDir "codex.exe") -Destination (Join-Path $InstallDir "codex.exe")
Install-Binary -Source (Join-Path $TargetDir "qwen-codex.exe") -Destination (Join-Path $InstallDir "qwen-codex-real.exe")
Install-Binary -Source (Join-Path $TargetDir "qwencodex.exe") -Destination (Join-Path $InstallDir "qwencodex-real.exe")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $InstallDir "qwen-codex.exe")
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $InstallDir "qwencodex.exe")
Write-QwenCmdWrapper -Name "qwen-codex" -RealExe "qwen-codex-real.exe"
Write-QwenCmdWrapper -Name "qwencodex" -RealExe "qwencodex-real.exe"

Write-Host "Installed qwen-codex commands into: $InstallDir"
Write-Host "Configured Qwen endpoint: $BaseUrl"
Write-Host "Configured Qwen model: $Model"
if (($env:Path -split ";") -notcontains $InstallDir) {
    Write-Host "Add this directory to PATH, then open a new PowerShell:"
    Write-Host "  `$env:Path = `"$InstallDir;`$env:Path`""
}
Write-Host "Run: qwen-codex"
