param(
    [string]$InstallDir = "",
    [string]$ExpectedVersion = "0.1.2"
)
$ErrorActionPreference = "Stop"

$failed = $false
function Check($label, $condition, $detail) {
    if ($condition) {
        Write-Host "[OK] $label - $detail" -ForegroundColor Green
    } else {
        Write-Host "[FAIL] $label - $detail" -ForegroundColor Red
        $script:failed = $true
    }
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $installedExe = Get-ChildItem -Path $env:LOCALAPPDATA -Filter billiards_matrix.exe -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($installedExe) { $InstallDir = $installedExe.Directory.FullName }
}
$exe = Join-Path $InstallDir "billiards_matrix.exe"
$icon = Join-Path $InstallDir "icons\icon.ico"
$tunnelKey = Join-Path $InstallDir "resources\tunnel\billiards_tunnel_ed25519"
$tunnelSsh = Join-Path $InstallDir "resources\tunnel\ssh.exe"
Check "Install directory" (Test-Path $InstallDir) $InstallDir
Check "Application" (Test-Path $exe) $exe
Check "Application icon" (Test-Path $icon) $icon
Check "Embedded tunnel key" (Test-Path $tunnelKey) $tunnelKey
Check "Embedded SSH" (Test-Path $tunnelSsh) $tunnelSsh

if (Test-Path $exe) {
    $version = (Get-Item $exe).VersionInfo.ProductVersion
    Check "Application version" ($version -like "$ExpectedVersion*") $version
}

$webview = Get-Process msedgewebview2 -ErrorAction SilentlyContinue | Select-Object -First 1
Check "WebView2 runtime" ($null -ne $webview) "msedgewebview2.exe"

$app = Get-Process billiards_matrix -ErrorAction SilentlyContinue | Select-Object -First 1
if ($app) {
    $children = Get-CimInstance Win32_Process | Where-Object ParentProcessId -eq $app.Id
    $childNames = @($children | Select-Object -ExpandProperty Name)
    Check "No CMD child process" (-not ($childNames -contains "cmd.exe")) ($childNames -join ", ")
    $unexpected = @($childNames | Where-Object { $_ -notin @("msedgewebview2.exe", "ssh.exe") })
    Check "Allowed child processes only" ($unexpected.Count -eq 0) ($childNames -join ", ")
} else {
    Write-Host '[INFO] Application is not running; child process checks skipped' -ForegroundColor Yellow
}

if ($failed) { exit 1 }
Write-Host 'Windows release checks passed. Install/uninstall workflow still requires manual clean-machine confirmation.' -ForegroundColor Green
