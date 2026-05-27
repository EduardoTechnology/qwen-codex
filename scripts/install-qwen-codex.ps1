param(
    [switch]$Release,
    [string]$InstallDir = (Join-Path $env:USERPROFILE ".cargo\bin")
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$ProfileDir = if ($Release) { "release" } else { "debug" }
$CargoArgs = @("build", "-p", "codex-cli", "--bins")
if ($Release) {
    $CargoArgs += "--release"
}

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
    Copy-Item -Force -Path $Source -Destination (Join-Path $InstallDir "$Bin.exe")
}

Write-Host "Installed qwen-codex commands into: $InstallDir"
if (($env:Path -split ";") -notcontains $InstallDir) {
    Write-Host "Add this directory to PATH, then open a new PowerShell:"
    Write-Host "  `$env:Path = `"$InstallDir;`$env:Path`""
}
Write-Host "Run: qwen-codex"
