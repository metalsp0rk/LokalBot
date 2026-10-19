# First step of a job on a self-hosted Windows runner (setup-windows-runner.ps1):
# - fails if the runner can reach the Tailscale network, which its firewall
#   rule is meant to block;
# - lowers the job's CPU priority, so builds don't take the PC from whoever
#   is using it (later steps inherit it from the runner's worker process);
# - clears the warm target directory when the disk gets too full.
$ErrorActionPreference = 'Stop'

$client = New-Object Net.Sockets.TcpClient
try {
  $reached = $false
  try { $reached = $client.ConnectAsync('100.100.100.100', 53).Wait(3000) } catch [AggregateException] { }
  if ($reached) {
    throw 'This runner can reach the Tailscale network. Check the firewall rule from setup-windows-runner.ps1.'
  }
} finally {
  $client.Dispose()
}

$worker = (Get-CimInstance Win32_Process -Filter "ProcessId = $PID").ParentProcessId
(Get-Process -Id $worker).PriorityClass = 'BelowNormal'
Write-Host "Steps now run at below-normal priority (worker $worker)."

$target = $env:CARGO_TARGET_DIR
if (-not $target) { throw 'The runner sets CARGO_TARGET_DIR in its .env.' }
New-Item -ItemType Directory -Force $target | Out-Null
$drive = Get-PSDrive ((Resolve-Path $target).Drive.Name)
$freeGb = [int]($drive.Free / 1GB)
Write-Host "Warm target in $target; $freeGb GB free on $($drive.Name):."
$minFree = if ($env:MIN_FREE_GB) { [int]$env:MIN_FREE_GB } else { 40 }
if ($freeGb -lt $minFree) {
  Write-Host 'Clearing the warm target to free space.'
  Get-ChildItem -Force $target | Remove-Item -Recurse -Force
}
