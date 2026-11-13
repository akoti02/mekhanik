# Mekhanik - GPU POWER CLAMP HEALING (13.11.2026). ASCII only: Windows PowerShell 5.1 reads a file without BOM as ANSI.
#
# CLAMP: on mains power the NVIDIA power limit is below 60 % of the default (13.11 11:42-15:36: 33 of 80 W, P5, memory
# 810 of 14001 MHz) - everything computing on the card runs several times slower. It sticks after charging from an
# almost empty battery; before 13.11 only re-plugging the charger or a reboot released it. On other NVIDIA laptops the
# same stuck state (P5, 810 MHz, "SW Power Cap", charger plugged in under load) is released by restarting Dynamic Boost -
# nvidia-powerd on Linux (forums.developer.nvidia.com 375678, 382741); on Windows its place is the device "NVIDIA
# Platform Controllers and Framework" (nvpcf). Ladder: (1) restart nvpcf, (2) restart service
# NVDisplay.ContainerLocalSystem. Measure $Zhdat s after each step; released - stop.
#
# WHO CALLS: scheduled task Mekhanik-Videokarta (SYSTEM) on event 4201 of source "Mekhanik" in the Application log -
# Mekhanik writes it when the card has been clamped for a minute on mains power. The event is only a reason: the script
# measures the clamp itself and touches nothing without it; heals at most once per 8 min. It reads and writes only its
# own folder (C:\ProgramData\Mekhanik: SYSTEM and administrators write, users read) - nothing in user folders.
# RESULT: zazhim.log and zazhim_itog.json there; Mekhanik reads the result and writes its journal.
#
#   -Proba : restart nvpcf once even without a clamp (installer: the restart does not break anything)
#   -Suho  : restart nothing (logic check); -Zhdat - seconds after a step (20)
#   -Podmena 'limit,default,P,mains;...' : measurements in turn instead of nvidia-smi (only with -Suho)
param([switch]$Proba, [switch]$Suho, [int]$Zhdat = 20, [string]$Podmena = '',
      [string]$Dir = (Join-Path $env:ProgramData 'Mekhanik'))
$ErrorActionPreference = 'Continue'
$log = Join-Path $Dir 'zazhim.log'
$pamyat = Join-Path $Dir 'zazhim_pamyat.txt'
$itogPut = Join-Path $Dir 'zazhim_itog.json'
$imya = 'NVIDIA Platform Controllers and Framework'
$dev = 'ACPI\NVDA0820\NPCF'
$sluzhba = 'NVDisplay.ContainerLocalSystem'
$dolya = 0.6
$neChashcheMin = 8
$inv = [Globalization.CultureInfo]::InvariantCulture

if (-not (Test-Path -LiteralPath $Dir)) { New-Item -ItemType Directory -Force -Path $Dir | Out-Null }
# the log does not grow forever
try { if ((Get-Item -LiteralPath $log -ErrorAction Stop).Length -gt 512KB) { Move-Item -LiteralPath $log -Destination "$log.old" -Force } } catch {}

function Say([string]$m) {
  $line = ('{0}  {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $m)
  Write-Output $line
  try { Add-Content -Path $log -Value $line -Encoding ascii -ErrorAction Stop } catch {}
}

# "33.00, 80.00, P5" -> limit 33, default 80, P5; "[N/A]" or no field - $null
function Razobrat([string]$s) {
  $ch = @(($s -split ',') | ForEach-Object { $_.Trim() })
  $v = @{ predel = $null; obychnyy = $null; p = $null; set = $null }
  $x = 0.0
  if ($ch.Count -ge 1 -and [double]::TryParse($ch[0], [Globalization.NumberStyles]::Float, $inv, [ref]$x) -and $x -gt 0) { $v.predel = [int][math]::Round($x) }
  if ($ch.Count -ge 2 -and [double]::TryParse($ch[1], [Globalization.NumberStyles]::Float, $inv, [ref]$x) -and $x -gt 0) { $v.obychnyy = [int][math]::Round($x) }
  if ($ch.Count -ge 3 -and $ch[2] -match '^P\d{1,2}$') { $v.p = $ch[2] }
  $v
}

$script:podmeny = [System.Collections.Queue]::new()
if ($Suho -and $Podmena) { foreach ($p in ($Podmena -split ';')) { $script:podmeny.Enqueue($p) } }

function Zamer {
  if ($script:podmeny.Count) {
    $ch = ([string]$script:podmeny.Dequeue()) -split ','
    $z = Razobrat (($ch | Select-Object -First 3) -join ',')
    if ($ch.Count -ge 4) { $z.set = ($ch[3].Trim() -eq 'True') }
    return $z
  }
  $smi = Join-Path $env:SystemRoot 'System32\nvidia-smi.exe'
  $s = ''
  try { $s = [string](& $smi --query-gpu=enforced.power.limit,power.default_limit,pstate --format=csv,noheader,nounits 2>$null | Select-Object -First 1) } catch {}
  $z = Razobrat $s
  try { $z.set = [bool]((Get-CimInstance -Namespace root\wmi -ClassName BatteryStatus -ErrorAction Stop | Select-Object -First 1).PowerOnline) } catch {}
  $z
}

# clamped: on mains and the limit below the share of the default; unknown - not clamped
function Zazhata($z) { ($z.set -eq $true) -and ($null -ne $z.predel) -and ($null -ne $z.obychnyy) -and ($z.predel -lt $z.obychnyy * $dolya) }

# the device by its id on this laptop, otherwise by its name (another model - another id)
function Ustroystvo {
  $d = $null
  try { $d = Get-PnpDevice -InstanceId $dev -ErrorAction Stop } catch {}
  if (-not $d) { try { $d = Get-PnpDevice -FriendlyName $imya -ErrorAction Stop | Select-Object -First 1 } catch {} }
  $d
}

# result for Mekhanik: written whole (temp file, then replace) - Mekhanik never reads half of it
function Napisat-Itog([int]$shag, [string]$ishod, $z0, $z1) {
  $o = [ordered]@{ kogda = (Get-Date).ToUniversalTime().ToString('o'); shag = $shag; itog = $ishod
                   predel_do = $z0.predel; predel_posle = $(if ($z1) { $z1.predel } else { $z0.predel })
                   obychnyy = $z0.obychnyy; p_do = $z0.p; p_posle = $(if ($z1) { $z1.p } else { $z0.p })
                   proba = [bool]$Proba; suho = [bool]$Suho; versiya = 1 }
  $tmp = "$itogPut.tmp"
  try {
    ($o | ConvertTo-Json -Compress) | Set-Content -Path $tmp -Encoding ascii -ErrorAction Stop
    Move-Item -LiteralPath $tmp -Destination $itogPut -Force -ErrorAction Stop
  } catch { Say "result not written: $_" }
}

# --- not more often than once per $neChashcheMin min
if (-not $Proba -and (Test-Path -LiteralPath $pamyat)) {
  try {
    $bylo = [datetime]::Parse((Get-Content -LiteralPath $pamyat -Raw).Trim(), $inv, [Globalization.DateTimeStyles]::RoundtripKind)
    if (((Get-Date) - $bylo).TotalMinutes -lt $neChashcheMin) {
      Say "request: healed less than $neChashcheMin min ago - skip"
      Napisat-Itog 0 'chasto' @{} $null
      exit 0
    }
  } catch {}
}
$z0 = Zamer
Say ('measure: limit {0} of {1} W, {2}, on mains {3}' -f $z0.predel, $z0.obychnyy, $z0.p, $z0.set)
if (-not $Proba -and -not (Zazhata $z0)) {
  Say 'no clamp - nothing to do'
  Napisat-Itog 0 'ne_nuzhno' $z0 $null
  exit 0
}
try { Set-Content -LiteralPath $pamyat -Value ((Get-Date).ToString('o')) -Encoding ascii -ErrorAction Stop } catch {}

# --- step 1: restart nvpcf (Dynamic Boost)
$u = Ustroystvo
$id = if ($u) { [string]$u.InstanceId } else { $dev }
Say "step 1: restart device $id$(if ($Suho) { ' (dry)' })"
if (-not $Suho) {
  $o = (& pnputil.exe /restart-device $id 2>&1 | Out-String).Trim()
  Say ('  pnputil: ' + ($o -replace '\s+', ' '))
}
Start-Sleep -Seconds $Zhdat
$st = $null
try { $st = [string](Get-PnpDevice -InstanceId $id -ErrorAction Stop).Status } catch {}
if ($st -and $st -ne 'OK' -and -not $Suho) {
  Say "  device after restart: $st - enabling"
  & pnputil.exe /enable-device $id 2>&1 | Out-Null
  Start-Sleep -Seconds 5
}
$z1 = Zamer
Say ('  after step 1: limit {0} of {1} W, {2}, device {3}' -f $z1.predel, $z1.obychnyy, $z1.p, $st)
if ($Proba) { Napisat-Itog 1 'proba' $z0 $z1; Say 'probe: done'; exit 0 }
if (-not (Zazhata $z1)) { Say 'RELEASED by step 1'; Napisat-Itog 1 'snyat' $z0 $z1; exit 0 }

# --- step 2: restart service NVIDIA Display Container LS
Say "step 2: restart service $sluzhba$(if ($Suho) { ' (dry)' })"
if (-not $Suho) { try { Restart-Service -Name $sluzhba -Force -ErrorAction Stop } catch { Say "  failed: $_" } }
Start-Sleep -Seconds $Zhdat
$z2 = Zamer
Say ('  after step 2: limit {0} of {1} W, {2}' -f $z2.predel, $z2.obychnyy, $z2.p)
if (-not (Zazhata $z2)) { Say 'RELEASED by step 2'; Napisat-Itog 2 'snyat' $z0 $z2; exit 0 }
Say 'NOT RELEASED: only the charger (out for 10 s) or a reboot helps'
Napisat-Itog 2 'ne_snyat' $z0 $z2
exit 0
