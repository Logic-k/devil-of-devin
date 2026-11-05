$procs = Get-CimInstance Win32_Process -Filter "Name='devin.exe'" |
  Where-Object { $_.CommandLine -match 'auth\s+login' }
foreach ($p in $procs) {
  Write-Output ("killing devin auth login PID " + $p.ProcessId + "  parent=" + $p.ParentProcessId)
  Stop-Process -Id $p.ProcessId -Force
}
if (-not $procs) { Write-Output "no stuck login" }
