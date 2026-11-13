# Mekhanik - COMPATIBILITY SHIM (13.11.2026). ASCII only. The helper with system rights moved from videokarta\ to
# pomoshchnik\ and now also sets up audio healing (event 4202) and the firewall lock. This shim forwards to the new
# place so old notes and commands keep working.
#   powershell -ExecutionPolicy Bypass -File videokarta\ustanovit.ps1 [-Snyat] [-BezProby] [-Suho]
param([switch]$BezProby, [switch]$Snyat, [switch]$Suho, [string]$UserLocalAppData = '', [string]$SuhoDir = '')
$novyy = Join-Path (Join-Path (Split-Path $PSScriptRoot -Parent) 'pomoshchnik') 'ustanovit.ps1'
Write-Output "videokarta\ustanovit.ps1 moved to pomoshchnik\ustanovit.ps1 - forwarding"
$a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $novyy)
if ($BezProby) { $a += '-BezProby' }
if ($Snyat) { $a += '-Snyat' }
if ($Suho) { $a += '-Suho' }
if ($UserLocalAppData) { $a += @('-UserLocalAppData', $UserLocalAppData) }
if ($SuhoDir) { $a += @('-SuhoDir', $SuhoDir) }
& powershell.exe @a
exit $LASTEXITCODE
