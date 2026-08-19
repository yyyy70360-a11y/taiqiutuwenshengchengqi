param(
    [string]$ResourceDir = "",
    [int]$LocalPort = 38124
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($ResourceDir)) {
    $ResourceDir = Join-Path $PSScriptRoot "..\..\src-tauri\resources\tunnel"
}
$ssh = Join-Path $ResourceDir "ssh.exe"
$key = Join-Path $ResourceDir "billiards_tunnel_ed25519"
if (-not (Test-Path -LiteralPath $ssh)) { throw "Embedded SSH not found: $ssh" }
if (-not (Test-Path -LiteralPath $key)) { throw "Embedded tunnel key not found: $key" }

$knownHosts = Join-Path ([System.IO.Path]::GetTempPath()) ("billiards-known-hosts-{0}.txt" -f ([guid]::NewGuid().ToString("N")))
$arguments = @(
    "-N", "-L", ("{0}:127.0.0.1:38123" -f $LocalPort),
    "-o", "ExitOnForwardFailure=yes",
    "-o", "ServerAliveInterval=30",
    "-o", "ServerAliveCountMax=3",
    "-o", "StrictHostKeyChecking=accept-new",
    "-o", ("UserKnownHostsFile={0}" -f $knownHosts),
    "-o", "IdentitiesOnly=yes",
    "-i", $key,
    "billiards-tunnel@115.191.33.129"
)

$process = $null
try {
    $process = Start-Process -FilePath $ssh -ArgumentList $arguments -WindowStyle Hidden -PassThru
    $health = $null
    for ($attempt = 0; $attempt -lt 20 -and $null -eq $health; $attempt++) {
        Start-Sleep -Milliseconds 500
        if ($process.HasExited) { throw "SSH tunnel exited before health check" }
        try {
            $candidate = Invoke-RestMethod -Uri ("http://127.0.0.1:{0}/health" -f $LocalPort) -TimeoutSec 2
            if ($candidate.status -eq "ok") { $health = $candidate }
        } catch {
            # The listener can take a moment to bind after SSH authenticates.
        }
    }
    if ($null -eq $health) { throw "Tunnel health check did not return status=ok" }
    Write-Host ("[OK] SSH tunnel health: {0}" -f ($health | ConvertTo-Json -Compress)) -ForegroundColor Green
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        $process.WaitForExit(3000) | Out-Null
    }
    Remove-Item -LiteralPath $knownHosts -Force -ErrorAction SilentlyContinue
}

if ($null -ne $process -and -not $process.HasExited) { throw "SSH tunnel process was not reaped" }
Write-Host "[OK] SSH tunnel process reaped" -ForegroundColor Green
