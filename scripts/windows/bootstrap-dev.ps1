$ErrorActionPreference = "Stop"

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-Administrator)) {
    $scriptPath = $MyInvocation.MyCommand.Path
    Start-Process powershell.exe -Verb RunAs -ArgumentList @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", ('"' + $scriptPath + '"')
    )
    exit
}

if (-not (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
    throw "winget is required. Install App Installer from Microsoft Store first."
}

Write-Host "Updating winget sources..."
winget source update --disable-interactivity

Write-Host "Installing Git..."
winget install --id Git.Git --exact --silent --accept-package-agreements --accept-source-agreements --disable-interactivity

Write-Host "Installing Visual Studio 2022 C++ Build Tools and Windows SDK..."
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --silent `
    --accept-package-agreements --accept-source-agreements --disable-interactivity `
    --override "--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

Write-Host "Installing Rust MSVC toolchain..."
winget install --id Rustlang.Rustup --exact --silent --accept-package-agreements --accept-source-agreements --disable-interactivity

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
$rustup = Join-Path $cargoBin "rustup.exe"
$cargo = Join-Path $cargoBin "cargo.exe"
$env:Path = "$cargoBin;$env:Path"

if (-not (Test-Path $rustup)) {
    throw "rustup was not installed at $rustup"
}

& $rustup default stable-msvc
& $rustup target add x86_64-pc-windows-msvc

Write-Host "Installing Tauri CLI 2.11.4..."
& $cargo install tauri-cli --version 2.11.4 --locked

$webViewRoot = Join-Path ${env:ProgramFiles(x86)} "Microsoft\EdgeWebView\Application"
if (-not (Test-Path $webViewRoot)) {
    Write-Host "Installing Microsoft Edge WebView2 Runtime..."
    $webViewInstaller = Join-Path $env:TEMP "MicrosoftEdgeWebview2Setup.exe"
    Invoke-WebRequest "https://go.microsoft.com/fwlink/p/?LinkId=2124703" -OutFile $webViewInstaller
    Start-Process $webViewInstaller -ArgumentList "/silent", "/install" -Wait
}

Write-Host "Windows Tauri development prerequisites are installed."
Write-Host "Restart Windows before the first production build."

