$ErrorActionPreference = "Stop"

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

$logPath = Join-Path $env:TEMP "billiards-bootstrap-dev.log"
Start-Transcript -Path $logPath -Force | Out-Null

if (-not (Get-Command git.exe -ErrorAction SilentlyContinue)) {
    Write-Host "Git is not available in the elevated PATH; continuing with the existing workspace."
}

function Restart-AsAdministrator {
    $scriptPath = $MyInvocation.ScriptName
    if (-not $scriptPath) {
        $scriptPath = $PSCommandPath
    }
    Stop-Transcript | Out-Null
    Start-Process powershell.exe -Verb RunAs -ArgumentList @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", ('"' + $scriptPath + '"')
    ) -Wait
    exit
}

function Add-UserPathEntry($pathEntry) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = @()
    if ($userPath) {
        $entries = $userPath -split ";" | Where-Object { $_ }
    }
    $alreadyPresent = $entries | Where-Object { $_.TrimEnd("\") -ieq $pathEntry.TrimEnd("\") }
    if (-not $alreadyPresent) {
        $newUserPath = (@($entries) + $pathEntry) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
        Write-Host "Added $pathEntry to the current user's PATH."
    }
}

$visualStudioRoots = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\2022"),
    (Join-Path $env:ProgramFiles "Microsoft Visual Studio\2022")
) | Where-Object { $_ -and (Test-Path $_) }
$compiler = if ($visualStudioRoots) {
    Get-ChildItem $visualStudioRoots -Filter cl.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object FullName -Match "Hostx64\\x64" |
        Select-Object -First 1
} else {
    $null
}

if (-not $compiler) {
    if (-not (Test-Administrator)) {
        Write-Host "Visual Studio Build Tools require administrator permission. Restarting elevated..."
        Restart-AsAdministrator
    }
    Write-Host "Installing Visual Studio 2022 C++ Build Tools and Windows SDK..."
    $buildToolsInstaller = Join-Path $env:TEMP "vs_BuildTools.exe"
    Invoke-WebRequest "https://aka.ms/vs/17/release/vs_BuildTools.exe" -OutFile $buildToolsInstaller
    $buildToolsProcess = Start-Process $buildToolsInstaller -ArgumentList @(
        "--wait", "--quiet", "--norestart",
        "--add", "Microsoft.VisualStudio.Workload.VCTools",
        "--includeRecommended"
    ) -PassThru -Wait
    if ($buildToolsProcess.ExitCode -notin @(0, 3010)) {
        throw "Visual Studio Build Tools installation failed with exit code $($buildToolsProcess.ExitCode)."
    }
}

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
$rustup = Join-Path $cargoBin "rustup.exe"
$cargo = Join-Path $cargoBin "cargo.exe"
$env:Path = "$cargoBin;$env:Path"
Add-UserPathEntry $cargoBin

if (-not (Test-Path $rustup)) {
    Write-Host "Installing Rust MSVC toolchain..."
    $rustupInstaller = Join-Path $env:TEMP "rustup-init.exe"
    Invoke-WebRequest "https://win.rustup.rs/x86_64" -OutFile $rustupInstaller
    $rustupProcess = Start-Process $rustupInstaller -ArgumentList @(
        "-y", "--no-modify-path", "--profile", "minimal",
        "--default-host", "x86_64-pc-windows-msvc",
        "--default-toolchain", "stable"
    ) -PassThru -Wait
    if ($rustupProcess.ExitCode -ne 0) {
        throw "Rust installation failed with exit code $($rustupProcess.ExitCode)."
    }
}

& $rustup default stable
& $rustup target add x86_64-pc-windows-msvc

Write-Host "Installing Tauri CLI 2.11.4..."
& $cargo install tauri-cli --version 2.11.4 --locked

$webViewRoots = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft\EdgeWebView\Application"),
    (Join-Path $env:ProgramFiles "Microsoft\EdgeWebView\Application")
) | Where-Object { $_ -and (Test-Path $_) }
if (-not $webViewRoots) {
    if (-not (Test-Administrator)) {
        Write-Host "Microsoft Edge WebView2 Runtime requires administrator permission. Restarting elevated..."
        Restart-AsAdministrator
    }
    Write-Host "Installing Microsoft Edge WebView2 Runtime..."
    $webViewInstaller = Join-Path $env:TEMP "MicrosoftEdgeWebview2Setup.exe"
    Invoke-WebRequest "https://go.microsoft.com/fwlink/p/?LinkId=2124703" -OutFile $webViewInstaller
    Start-Process $webViewInstaller -ArgumentList "/silent", "/install" -Wait
}

Write-Host "Windows Tauri development prerequisites are installed."
Write-Host "Restart Windows before the first production build."
Stop-Transcript | Out-Null

