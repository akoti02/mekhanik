# Mekhanik - AUDIO LEAK HEALING (13.11.2026, wave 2a package "zhizn"). ASCII only: PowerShell 5.1 reads a file without
# BOM as ANSI.
#
# LEAK: audiodg.exe (Windows Audio Device Graph) sometimes grows its handles/threads without end; in the worst case audio
# stutters or dies. Healing: restart the service "Windows Audio Endpoint Builder" (AudioEndpointBuilder) - it pulls the
# audio service with it, so sound cuts out for 1-2 s. Therefore ONLY when nothing is playing.
#
# WHO CALLS: scheduled task Mekhanik-Zvuk (SYSTEM) on event 4202 of source "Mekhanik" in the Application log - Mekhanik
# writes it when it sees a real leak trend (not the noisy normal counts of this PC: ~1300-1540 handles, ~110 threads -
# report N15). The event is only a reason: this script checks by itself and touches nothing unless:
#   1) the counts are clearly abnormal (well above the observed noise), and
#   2) nothing is playing (audiodg is near-idle over a 1 s sample - a rendering stream keeps it busy).
# Heals at most once per $neChashcheMin min. Reads and writes only its own folder (C:\ProgramData\Mekhanik).
# RESULT: zvuk.log and zvuk_itog.json there; Mekhanik reads the result and writes its journal.
#
#   -Suho   : restart nothing (logic check)
#   -Podmena 'handles,threads,cpu' : measurement instead of live audiodg (only with -Suho)
param([switch]$Suho, [string]$Podmena = '', [int]$neChashcheMin = 60,
      [string]$Dir = (Join-Path $env:ProgramData 'Mekhanik'))
$ErrorActionPreference = 'Continue'
$log = Join-Path $Dir 'zvuk.log'
$pamyat = Join-Path $Dir 'zvuk_pamyat.txt'
$itogPut = Join-Path $Dir 'zvuk_itog.json'
$sluzhba = 'AudioEndpointBuilder'              # Windows Audio Endpoint Builder (Audiosrv depends on it)
# above the observed noise of this PC (1331-1543 handles, 108-119 threads on 12.11) with a wide margin
$porogDeskriptorov = 3000
$porogPotokov = 300
$zanyatCpu = 1.0                               # audiodg busier than this over the sample -> sound is playing
$inv = [Globalization.CultureInfo]::InvariantCulture

if (-not (Test-Path -LiteralPath $Dir)) { New-Item -ItemType Directory -Force -Path $Dir | Out-Null }
try { if ((Get-Item -LiteralPath $log -ErrorAction Stop).Length -gt 512KB) { Move-Item -LiteralPath $log -Destination "$log.old" -Force } } catch {}

function Say([string]$m) {
  $line = ('{0}  {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $m)
  Write-Output $line
  try { Add-Content -Path $log -Value $line -Encoding ascii -ErrorAction Stop } catch {}
}

# measure audiodg: handles, threads, cpu% over 1 s; -Suho with -Podmena substitutes the three numbers
function Zamer {
  if ($Suho -and $Podmena) {
    $ch = @(($Podmena -split ',') | ForEach-Object { $_.Trim() })
    return @{ deskriptory = [int]$ch[0]; potoki = [int]$ch[1]; cpu = [double]::Parse($ch[2], $inv); est = $true }
  }
  $p = Get-Process -Name audiodg -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $p) { return @{ deskriptory = 0; potoki = 0; cpu = 0.0; est = $false } }
  # 14.11 (audit): audiodg can exit (Windows recycles it on device changes) during the 1 s sample - then the reads
  # below throw; answer 'no audiodg' instead of dying without a result file
  try {
    $c1 = $p.TotalProcessorTime
    if (-not $Suho) { Start-Sleep -Milliseconds 1000 }
    $p.Refresh()
    $c2 = $p.TotalProcessorTime
    $cpu = (($c2 - $c1).TotalMilliseconds / (1000.0 * [Environment]::ProcessorCount)) * 100.0
    @{ deskriptory = [int]$p.HandleCount; potoki = [int]$p.Threads.Count; cpu = [math]::Round($cpu, 1); est = $true }
  } catch {
    @{ deskriptory = 0; potoki = 0; cpu = 0.0; est = $false }
  }
}

# a real leak worth a restart: clearly abnormal counts
function Utechka($z) { $z.est -and (($z.deskriptory -ge $porogDeskriptorov) -or ($z.potoki -ge $porogPotokov)) }
# sound is playing: audiodg is doing DSP work
function Zvuk-Igraet($z) { $z.est -and ($z.cpu -ge $zanyatCpu) }

function Napisat-Itog([string]$ishod, $z) {
  $o = [ordered]@{ kogda = (Get-Date).ToUniversalTime().ToString('o'); itog = $ishod
                   deskriptory = $z.deskriptory; potoki = $z.potoki; cpu = $z.cpu
                   suho = [bool]$Suho; versiya = 1 }
  $tmp = "$itogPut.tmp"
  try {
    ($o | ConvertTo-Json -Compress) | Set-Content -Path $tmp -Encoding ascii -ErrorAction Stop
    Move-Item -LiteralPath $tmp -Destination $itogPut -Force -ErrorAction Stop
  } catch { Say "result not written: $_" }
}

# --- not more often than once per $neChashcheMin min
if (Test-Path -LiteralPath $pamyat) {
  try {
    $bylo = [datetime]::Parse((Get-Content -LiteralPath $pamyat -Raw).Trim(), $inv, [Globalization.DateTimeStyles]::RoundtripKind)
    if (((Get-Date) - $bylo).TotalMinutes -lt $neChashcheMin) {
      Say "request: restarted audio less than $neChashcheMin min ago - skip"
      Napisat-Itog 'chasto' @{ deskriptory = 0; potoki = 0; cpu = 0.0 }
      exit 0
    }
  } catch {}
}

$z = Zamer
if (-not $z.est) { Say 'audiodg not found - nothing to do'; Napisat-Itog 'net_audiodg' $z; exit 0 }
Say ('measure: handles {0}, threads {1}, cpu {2}%' -f $z.deskriptory, $z.potoki, $z.cpu)
if (-not (Utechka $z)) { Say 'counts within normal - no leak'; Napisat-Itog 'net_utechki' $z; exit 0 }
if (Zvuk-Igraet $z) { Say 'sound is playing (audiodg busy) - not restarting'; Napisat-Itog 'zvuk_igraet' $z; exit 0 }

Say "leak and silence: restart service $sluzhba$(if ($Suho) { ' (dry)' })"
try { Set-Content -LiteralPath $pamyat -Value ((Get-Date).ToString('o')) -Encoding ascii -ErrorAction Stop } catch {}
if (-not $Suho) { try { Restart-Service -Name $sluzhba -Force -ErrorAction Stop } catch { Say "  failed: $_" } }
Napisat-Itog 'perezapushcheno' $z
Say 'audio service restarted (sound returns in 1-2 s)'
exit 0
