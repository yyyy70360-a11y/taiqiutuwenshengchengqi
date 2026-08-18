param(
    [switch]$RestartExplorer
)

$ErrorActionPreference = "Stop"

$productName = -join ([char[]](0x53F0, 0x7403, 0x56FE, 0x6587, 0x751F, 0x6210, 0x5668))
$installDir = Join-Path $env:LOCALAPPDATA $productName
$sourceIcon = Join-Path $PSScriptRoot "..\..\src-tauri\icons\icon.ico"
$installedIcon = Join-Path $installDir "app.ico"

if (-not (Test-Path -LiteralPath $sourceIcon)) {
    throw "Cannot find source icon: $sourceIcon"
}

if (-not (Test-Path -LiteralPath $installDir)) {
    throw "Cannot find installed app directory: $installDir"
}

Copy-Item -LiteralPath $sourceIcon -Destination $installedIcon -Force

$shortcutPaths = @(
    (Join-Path ([Environment]::GetFolderPath("Desktop")) "$productName.lnk"),
    (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$productName.lnk")
)

$taskbarPath = Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"
if (Test-Path -LiteralPath $taskbarPath) {
    $shortcutPaths += Get-ChildItem -LiteralPath $taskbarPath -Filter "*.lnk" -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "*$productName*" } |
        Select-Object -ExpandProperty FullName
}

$shell = New-Object -ComObject WScript.Shell
$updated = @()

foreach ($shortcutPath in ($shortcutPaths | Where-Object { $_ } | Select-Object -Unique)) {
    if (-not (Test-Path -LiteralPath $shortcutPath)) {
        continue
    }

    $shortcut = $shell.CreateShortcut($shortcutPath)
    $shortcut.IconLocation = "$installedIcon,0"
    $shortcut.Save()
    $updated += $shortcutPath
}

ie4uinit.exe -show 2>$null
rundll32.exe user32.dll,UpdatePerUserSystemParameters 1, True

if ($RestartExplorer) {
    Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
    Start-Process explorer.exe
}

Write-Host "Installed icon: $installedIcon"
Write-Host "Updated shortcuts: $($updated.Count)"
foreach ($shortcutPath in $updated) {
    Write-Host "- $shortcutPath"
}
Write-Host "If a pinned taskbar icon still looks old, unpin it and pin the Start Menu shortcut again."
