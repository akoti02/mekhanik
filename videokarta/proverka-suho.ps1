# Mekhanik - dry check of zazhim-lechit.ps1 (13.11.2026). ASCII only. No administrator, nothing restarted (-Suho):
# measurements are substituted (-Podmena), the folder is a temporary one. Every case: the outcome and the step.
#   powershell -ExecutionPolicy Bypass -File C:\dev\mekhanik\videokarta\proverka-suho.ps1
$ErrorActionPreference = 'Stop'
$skript = Join-Path $PSScriptRoot 'zazhim-lechit.ps1'
$errs = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($skript, [ref]$null, [ref]$errs)
if ($errs -and $errs.Count) { Write-Output ('PARSE ERROR: ' + $errs[0].Message); exit 1 }
$bytes = [IO.File]::ReadAllBytes($skript)
if (@($bytes | Where-Object { $_ -gt 127 }).Count) { Write-Output 'NOT ASCII: PowerShell 5.1 would read it as ANSI'; exit 1 }

$plokho = 0
function Sluchay([string]$imya, [string]$podmena, [string]$zhdu, [int]$shag, [switch]$Proba, [switch]$Ostavit) {
  if (-not $Ostavit) { Remove-Item -Recurse -Force $script:papka -ErrorAction SilentlyContinue }
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $skript, '-Suho', '-Zhdat', '0', '-Podmena', $podmena, '-Dir', $script:papka)
  if ($Proba) { $a += '-Proba' }
  $null = & powershell.exe @a
  $itog = Get-Content -Raw (Join-Path $script:papka 'zazhim_itog.json') | ConvertFrom-Json
  $ok = ($itog.itog -eq $zhdu) -and ($itog.shag -eq $shag) -and $itog.suho
  Write-Output ('{0,-4} {1,-46} -> {2} shag {3}' -f $(if ($ok) { 'ok' } else { 'BAD' }), $imya, $itog.itog, $itog.shag)
  if (-not $ok) { $script:plokho++ }
}
$script:papka = Join-Path $env:TEMP ('mekhanik-suho-' + [guid]::NewGuid().ToString('N').Substring(0, 8))

Sluchay 'no clamp on mains (98 of 80 W)'          '98.00,80.00,P0,True' 'ne_nuzhno' 0
Sluchay 'battery: low limit is not a clamp'      '33.00,80.00,P5,False' 'ne_nuzhno' 0
Sluchay 'driver silent about the limit'          '[N/A],80.00,P5,True' 'ne_nuzhno' 0
Sluchay '55 of 80 W is not below 60 %'           '55.00,80.00,P4,True' 'ne_nuzhno' 0
Sluchay 'clamp released by step 1'               '33.00,80.00,P5,True;98.00,80.00,P0,True' 'snyat' 1
Sluchay 'clamp released by step 2'               '33.00,80.00,P5,True;33.00,80.00,P5,True;101.00,80.00,P0,True' 'snyat' 2
Sluchay 'clamp not released'                     '33.00,80.00,P5,True;33.00,80.00,P5,True;33.00,80.00,P5,True' 'ne_snyat' 2
Sluchay 'again within 8 min - skip'              '33.00,80.00,P5,True;33.00,80.00,P5,True' 'chasto' 0 -Ostavit
Sluchay 'probe restarts even without a clamp'    '98.00,80.00,P0,True;103.00,80.00,P0,True' 'proba' 1 -Proba -Ostavit
$log = Get-Content (Join-Path $script:papka 'zazhim.log')
Write-Output ('log lines in the last folder: {0}' -f $log.Count)
Remove-Item -Recurse -Force $script:papka -ErrorAction SilentlyContinue
if ($plokho) { Write-Output "=== BAD: $plokho"; exit 1 } else { Write-Output '=== all cases ok' }
