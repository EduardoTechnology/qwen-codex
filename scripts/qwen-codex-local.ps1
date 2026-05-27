param(
    [string]$BaseUrl = $env:QWEN_CODEX_BASE_URL,
    [string]$Prompt = "What is 2+2? Answer in one word.",
    [string]$Model = $env:QWEN_CODEX_MODEL,
    [string]$ApiKey = $env:QWEN_CODEX_API_KEY,
    [string]$ContextWindow = $env:QWEN_CODEX_CONTEXT_WINDOW,
    [string]$HealthTimeoutSec = $env:QWEN_CODEX_HEALTH_TIMEOUT_SECS
)

if ([string]::IsNullOrWhiteSpace($BaseUrl)) {
    $BaseUrl = "http://127.0.0.1:8002/v1"
}
if ([string]::IsNullOrWhiteSpace($ApiKey)) {
    $ApiKey = "local-dev-key"
}
if ([string]::IsNullOrWhiteSpace($ContextWindow)) {
    $ContextWindow = "32768"
}
if ([string]::IsNullOrWhiteSpace($HealthTimeoutSec)) {
    $HealthTimeoutSec = "10"
}

$HealthTimeoutSecValue = [int]$HealthTimeoutSec

$BaseUrl = $BaseUrl.TrimEnd("/")
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

function Detect-Model {
    if (-not [string]::IsNullOrWhiteSpace($Model)) {
        return $Model
    }
    try {
        $models = Invoke-RestMethod -Uri "$BaseUrl/models" -Method Get -TimeoutSec $HealthTimeoutSecValue -ErrorAction Stop
        if ($models.data -and $models.data.Count -gt 0 -and $models.data[0].id) {
            return $models.data[0].id
        }
    } catch {
        # Fall through to the documented local default.
    }
    return "qwen35-local"
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo was not found. Install Rust with rustup, then rerun this script: https://rustup.rs/"
    exit 1
}

$Model = Detect-Model

Write-Host "Building qwen-codex..."
Push-Location (Join-Path $RepoRoot "codex-rs")
try {
    cargo build -p codex-cli
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

$env:QWEN_CODEX_BASE_URL = $BaseUrl
$env:QWEN_CODEX_MODEL = $Model
$env:QWEN_CODEX_API_KEY = $ApiKey
$env:QWEN_CODEX_CONTEXT_WINDOW = $ContextWindow
$env:QWEN_CODEX_YOLO_REFINER_BASE_URL = if ($env:QWEN_CODEX_YOLO_REFINER_BASE_URL) { $env:QWEN_CODEX_YOLO_REFINER_BASE_URL } else { $BaseUrl }
$env:QWEN_CODEX_YOLO_REFINER_MODEL = if ($env:QWEN_CODEX_YOLO_REFINER_MODEL) { $env:QWEN_CODEX_YOLO_REFINER_MODEL } else { $Model }
$env:QWEN_CODEX_YOLO_REFINER_API_KEY = if ($env:QWEN_CODEX_YOLO_REFINER_API_KEY) { $env:QWEN_CODEX_YOLO_REFINER_API_KEY } else { $ApiKey }

$QwenBin = Join-Path $RepoRoot "codex-rs\target\debug\qwen-codex.exe"
if (-not (Test-Path $QwenBin)) {
    $QwenBin = Join-Path $RepoRoot "codex-rs\target\debug\qwen-codex"
}

Write-Host "Using model endpoint: $env:QWEN_CODEX_BASE_URL"
Write-Host "Using model name: $env:QWEN_CODEX_MODEL"
Write-Host "Checking model health..."
& $QwenBin --health
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

Write-Host ""
Write-Host "Running qwen-codex prompt..."
& $QwenBin $Prompt
exit $LASTEXITCODE
