$ErrorActionPreference = "Stop"
$failed = $false

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) {
    $env:Path = "$cargoBin;$env:Path"
}

function Test-Command($name, $arguments) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if (-not $command) {
        Write-Host "[MISSING] $name" -ForegroundColor Red
        $script:failed = $true
        return
    }
    $version = & $command.Source @arguments 2>&1 | Select-Object -First 1
    Write-Host "[OK] $name - $version" -ForegroundColor Green
}

Test-Command "git.exe" @("--version")
Test-Command "rustc.exe" @("--version")
Test-Command "cargo.exe" @("--version")
Test-Command "cargo.exe" @("tauri", "--version")

$buildToolsRoots = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\2022"),
    (Join-Path $env:ProgramFiles "Microsoft Visual Studio\2022")
) | Where-Object { $_ -and (Test-Path $_) }
$compiler = if ($buildToolsRoots) {
    Get-ChildItem $buildToolsRoots -Filter cl.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object FullName -Match "Hostx64\\x64" |
        Select-Object -First 1
} else {
    $null
}
if ($compiler) {
    Write-Host "[OK] MSVC compiler - $($compiler.FullName)" -ForegroundColor Green
} else {
    Write-Host "[MISSING] Visual Studio C++ Build Tools" -ForegroundColor Red
    $failed = $true
}

$webViewRoots = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft\EdgeWebView\Application"),
    (Join-Path $env:ProgramFiles "Microsoft\EdgeWebView\Application")
) | Where-Object { $_ -and (Test-Path $_) }
$webView = if ($webViewRoots) {
    Get-ChildItem $webViewRoots -Filter msedgewebview2.exe -Recurse -ErrorAction SilentlyContinue |
        Select-Object -First 1
} else {
    $null
}
if ($webView) {
    Write-Host "[OK] WebView2 Runtime - $($webView.Directory.Name)" -ForegroundColor Green
} else {
    Write-Host "[MISSING] WebView2 Runtime" -ForegroundColor Red
    $failed = $true
}

if ($failed) {
    exit 1
}

Write-Host "All Windows Tauri development prerequisites are ready." -ForegroundColor Green

