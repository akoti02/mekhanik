# Mekhanik - dry check of the helper's healers (13.11.2026). ASCII only. No administrator, nothing restarted (-Suho):
# measurements are substituted (-Podmena), the folder is a temporary one. Every case: the outcome (and, for the GPU
# clamp, the step).
#   powershell -ExecutionPolicy Bypass -File pomoshchnik\proverka-suho.ps1
$ErrorActionPreference = 'Stop'
$gpu = Join-Path $PSScriptRoot 'zazhim-lechit.ps1'
$zvuk = Join-Path $PSScriptRoot 'zvuk-lechit.ps1'

function Parse-Check([string]$path) {
  $errs = $null
  [void][System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$null, [ref]$errs)
  if ($errs -and $errs.Count) { Write-Output ("PARSE ERROR $($path): " + $errs[0].Message); exit 1 }
  $bytes = [IO.File]::ReadAllBytes($path)
  if (@($bytes | Where-Object { $_ -gt 127 }).Count) { Write-Output "NOT ASCII: $path"; exit 1 }
}
Parse-Check $gpu
Parse-Check $zvuk

$plokho = 0
$script:papka = Join-Path $env:TEMP ('mekhanik-suho-' + [guid]::NewGuid().ToString('N').Substring(0, 8))

# --- GPU clamp cases
function Gpu([string]$imya, [string]$podmena, [string]$zhdu, [int]$shag, [switch]$Proba, [switch]$Ostavit) {
  if (-not $Ostavit) { Remove-Item -Recurse -Force $script:papka -ErrorAction SilentlyContinue }
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $gpu, '-Suho', '-Zhdat', '0', '-Podmena', $podmena, '-Dir', $script:papka)
  if ($Proba) { $a += '-Proba' }
  $null = & powershell.exe @a
  $itog = Get-Content -Raw (Join-Path $script:papka 'zazhim_itog.json') | ConvertFrom-Json
  $ok = ($itog.itog -eq $zhdu) -and ($itog.shag -eq $shag) -and $itog.suho
  Write-Output ('{0,-4} GPU  {1,-44} -> {2} shag {3}' -f $(if ($ok) { 'ok' } else { 'BAD' }), $imya, $itog.itog, $itog.shag)
  if (-not $ok) { $script:plokho++ }
}
Gpu 'no clamp on mains (98 of 80 W)'        '98.00,80.00,P0,True' 'ne_nuzhno' 0
Gpu 'battery: low limit is not a clamp'    '33.00,80.00,P5,False' 'ne_nuzhno' 0
Gpu 'driver silent about the limit'        '[N/A],80.00,P5,True' 'ne_nuzhno' 0
Gpu '55 of 80 W is not below 60 %'         '55.00,80.00,P4,True' 'ne_nuzhno' 0
Gpu 'clamp released by step 1'             '33.00,80.00,P5,True;98.00,80.00,P0,True' 'snyat' 1
Gpu 'clamp released by step 2'             '33.00,80.00,P5,True;33.00,80.00,P5,True;101.00,80.00,P0,True' 'snyat' 2
Gpu 'clamp not released'                   '33.00,80.00,P5,True;33.00,80.00,P5,True;33.00,80.00,P5,True' 'ne_snyat' 2
Gpu 'again within 8 min - skip'            '33.00,80.00,P5,True;33.00,80.00,P5,True' 'chasto' 0 -Ostavit
Gpu 'probe restarts even without a clamp'  '98.00,80.00,P0,True;103.00,80.00,P0,True' 'proba' 1 -Proba -Ostavit

# --- audio leak cases (handles,threads,cpu)
function Zvuk([string]$imya, [string]$podmena, [string]$zhdu, [switch]$Ostavit) {
  if (-not $Ostavit) { Remove-Item -Recurse -Force $script:papka -ErrorAction SilentlyContinue }
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $zvuk, '-Suho', '-Podmena', $podmena, '-Dir', $script:papka)
  $null = & powershell.exe @a
  $itog = Get-Content -Raw (Join-Path $script:papka 'zvuk_itog.json') | ConvertFrom-Json
  $ok = ($itog.itog -eq $zhdu) -and $itog.suho
  Write-Output ('{0,-4} ZVUK {1,-44} -> {2}' -f $(if ($ok) { 'ok' } else { 'BAD' }), $imya, $itog.itog)
  if (-not $ok) { $script:plokho++ }
}
Zvuk 'normal counts, silence'              '1400,110,0.0' 'net_utechki'
Zvuk 'normal threads, silence'             '1500,119,0.0' 'net_utechki'
Zvuk 'leak but sound is playing'           '3200,120,8.5' 'zvuk_igraet'
Zvuk 'leak (threads) and silence'          '1200,320,0.0' 'perezapushcheno'
Zvuk 'leak (handles) and silence'          '3500,120,0.2' 'perezapushcheno'
Zvuk 'again within 60 min - skip'          '3600,130,0.0' 'chasto' -Ostavit

Remove-Item -Recurse -Force $script:papka -ErrorAction SilentlyContinue
if ($plokho) { Write-Output "=== BAD: $plokho"; exit 1 } else { Write-Output '=== all cases ok' }
