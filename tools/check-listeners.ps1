$procs = Get-Process devil-of-devin -ErrorAction SilentlyContinue
if (-not $procs) { Write-Output "app not running"; exit }
$procs | ForEach-Object { Write-Output ("PID " + $_.Id + "  " + $_.StartTime) }
foreach ($p in $procs) {
  $conns = Get-NetTCPConnection -State Listen -OwningProcess $p.Id -ErrorAction SilentlyContinue
  foreach ($c in $conns) { Write-Output ("  listen " + $c.LocalAddress + ":" + $c.LocalPort) }
}
