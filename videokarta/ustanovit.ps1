# Mekhanik - INSTALL GPU POWER CLAMP HEALING (13.11.2026). ASCII only. Once, as administrator (asks Windows itself).
#   powershell -ExecutionPolicy Bypass -File C:\dev\mekhanik\videokarta\ustanovit.ps1           # install / update
#   powershell -ExecutionPolicy Bypass -File C:\dev\mekhanik\videokarta\ustanovit.ps1 -Snyat    # remove
# Mekhanik itself runs without administrator rights; only this task has system rights, and it does one thing: on event
# 4201 of source "Mekhanik" (Mekhanik writes it when the card has been clamped a minute on mains power) it runs
# zazhim-lechit.ps1, which measures the clamp itself and touches nothing without it.
# What it does:
#   1) folder C:\ProgramData\Mekhanik owned by Administrators: SYSTEM and administrators write, users only read (a script
#      that runs with system rights cannot be replaced from a user session, and the task writes nothing elsewhere);
#   2) checks that zazhim-lechit.ps1 parses and puts it there (hash compared);
#   3) event source "Mekhanik" in the Application log (the event gets a readable text);
#   4) task Mekhanik-Videokarta: SYSTEM, on event 4201, also on batteries, one at a time, 5 min at most;
#   5) mark videokarta.json - Mekhanik knows the healing is installed;
#   6) before 13.11 the same job lived in BARS (task BARS-Videokarta, script in C:\ProgramData\BARS) - removed if present;
#   7) probe: (a) event 4201 as Mekhanik writes it -> the task measures by itself (no clamp - 'ne_nuzhno', nothing is
#      restarted): the whole chain event -> SYSTEM task -> script -> result; (b) one restart of "NVIDIA Platform
#      Controllers and Framework" without a clamp (-Proba: the device comes back, the limit is normal). -BezProby - no (b).
# Touches nothing else. -Snyat: task, folder, event source and the mark go.
param([switch]$BezProby, [switch]$Snyat)
$ErrorActionPreference = 'Continue'
$dir = Join-Path $env:ProgramData 'Mekhanik'
$log = Join-Path $dir 'ustanovka.log'
$task = 'Mekhanik-Videokarta'
$sobytie = 4201
$src = Join-Path $PSScriptRoot 'zazhim-lechit.ps1'
$dst = Join-Path $dir 'zazhim-lechit.ps1'
$metka = Join-Path $dir 'videokarta.json'
$itogPut = Join-Path $dir 'zazhim_itog.json'

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
         ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
  # raises itself: Windows asks "allow?" - that is his permission
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"")
  if ($BezProby) { $a += '-BezProby' }
  if ($Snyat) { $a += '-Snyat' }
  try { Start-Process powershell.exe -Verb RunAs -ArgumentList $a -ErrorAction Stop; Write-Output 'asked for administrator rights - see the Windows window' }
  catch { Write-Output 'ADMINISTRATOR NEEDED: permission not given' }
  exit 0
}

function Say([string]$m) {
  $line = ('{0}  {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $m)
  Write-Output $line
  if (Test-Path -LiteralPath $dir) { try { Add-Content -Path $log -Value $line -Encoding ascii } catch {} }
}

# the job before 13.11: BARS asked its own SYSTEM task (BARS-Videokarta) - now Mekhanik does it, the old one goes
function Snyat-Prezhnee {
  try { Unregister-ScheduledTask -TaskName 'BARS-Videokarta' -Confirm:$false -ErrorAction Stop; Say 'old task BARS-Videokarta removed (the job moved to Mekhanik)' }
  catch { Say 'old task BARS-Videokarta: none' }
  foreach ($f in 'zazhim-lechit.ps1', 'zazhim_pamyat.txt', 'zazhim.log') {
    $p = Join-Path $env:ProgramData "BARS\$f"
    if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue; Say "old $p removed" }
  }
}

if ($Snyat) {
  Say '=== removing GPU clamp healing ==='
  try { Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction Stop; Say "task $task removed" } catch { Say "task ${task}: none" }
  Snyat-Prezhnee
  try { if ([Diagnostics.EventLog]::SourceExists('Mekhanik')) { [Diagnostics.EventLog]::DeleteEventSource('Mekhanik'); Say 'event source Mekhanik removed' } } catch { Say "event source: $_" }
  Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
  Write-Output "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  folder $dir removed"
  Write-Output '=== DONE: healing removed (Mekhanik itself keeps running; install.ps1 -Remove removes it) ==='
  Start-Sleep -Seconds 10
  exit 0
}

$bad = 0
if (-not (Test-Path $src)) { Write-Output "REFUSED: no $src"; Start-Sleep 15; exit 1 }
# a broken script is not put in place: the task would run it with system rights
$errs = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($src, [ref]$null, [ref]$errs)
if ($errs -and $errs.Count) { Write-Output ('REFUSED: zazhim-lechit.ps1 does not parse: ' + $errs[0].Message); Start-Sleep 15; exit 1 }

# --- 1. folder: owner Administrators, no inherited rights; SYSTEM and Administrators full, Users read; children inherit
New-Item -ItemType Directory -Force -Path $dir | Out-Null
& icacls.exe $dir /setowner '*S-1-5-32-544' /T /C /Q 2>&1 | Out-Null
& icacls.exe $dir /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX' /C /Q 2>&1 | Out-Null
if (Get-ChildItem -LiteralPath $dir -Force -ErrorAction SilentlyContinue) { & icacls.exe "$dir\*" /reset /T /C /Q 2>&1 | Out-Null }
Say '=== installing GPU clamp healing ==='
$pravila = @((Get-Acl -LiteralPath $dir).Access)
# write data, append (create files and folders), delete, change rights, take ownership, generic all/write
$zapis = [int64](2 -bor 4 -bor 0x10000 -bor 0x40000 -bor 0x80000 -bor 0x10000000 -bor 0x40000000)
$pishut = @($pravila | Where-Object { $_.AccessControlType -eq 'Allow' -and (([int64][int]$_.FileSystemRights) -band $zapis) -ne 0 } |
  ForEach-Object { try { $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value } catch { [string]$_.IdentityReference } })
$chuzhie = @($pishut | Where-Object { $_ -ne 'S-1-5-18' -and $_ -ne 'S-1-5-32-544' })
$vladelec = (Get-Acl -LiteralPath $dir).GetOwner([Security.Principal.SecurityIdentifier]).Value
if ($chuzhie.Count -or $vladelec -ne 'S-1-5-32-544') { Say "ERROR folder rights: owner $vladelec, writers $($pishut -join ' ')"; $bad++ }
else { Say "folder ${dir}: owner Administrators, write - only SYSTEM and Administrators" }

# --- 2. script
Copy-Item $src $dst -Force
$a = (Get-FileHash $src -Algorithm SHA256).Hash
$b = (Get-FileHash $dst -Algorithm SHA256).Hash
if ($a -eq $b) { Say ('script put: ' + $dst + ' (' + $a.Substring(0, 12) + ')') } else { Say 'ERROR: script differs after copy'; $bad++ }

# --- 3. event source
try {
  if (-not [Diagnostics.EventLog]::SourceExists('Mekhanik')) { New-EventLog -LogName Application -Source 'Mekhanik' -ErrorAction Stop; Say 'event source Mekhanik registered (Application)' }
  else { Say 'event source Mekhanik: exists' }
} catch { Say "event source not registered (events still work): $_" }

# --- 4. task: SYSTEM, on event 4201 of Mekhanik
$xml = @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>Mekhanik</Author>
    <Description>Mekhanik: GPU power clamp healing on event $sobytie of Mekhanik (Application log) - $dst</Description>
  </RegistrationInfo>
  <Triggers>
    <EventTrigger>
      <Enabled>true</Enabled>
      <Subscription>&lt;QueryList&gt;&lt;Query Id="0" Path="Application"&gt;&lt;Select Path="Application"&gt;*[System[Provider[@Name='Mekhanik'] and (EventID=$sobytie)]]&lt;/Select&gt;&lt;/Query&gt;&lt;/QueryList&gt;</Subscription>
    </EventTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>S-1-5-18</UserId>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <ExecutionTimeLimit>PT5M</ExecutionTimeLimit>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>powershell.exe</Command>
      <Arguments>-NonInteractive -NoProfile -ExecutionPolicy Bypass -File "$dst"</Arguments>
    </Exec>
  </Actions>
</Task>
"@
try {
  Register-ScheduledTask -TaskName $task -Xml $xml -Force -ErrorAction Stop | Out-Null
  Say "task ${task}: registered"
} catch { Say "ERROR task: $_"; $bad++ }
# checked by the fact, not by the return code
try {
  $s = Get-ScheduledTask -TaskName $task -ErrorAction Stop
  $sub = [string]($s.Triggers | Select-Object -First 1).Subscription
  $ok = ($s.Principal.UserId -match 'SYSTEM') -and ($sub -match "EventID=$sobytie") -and (-not $s.Settings.DisallowStartIfOnBatteries) -and (-not $s.Settings.StopIfGoingOnBatteries)
  Say ("task ${task}: as {0}, on event {1}, also on batteries - {2}" -f $s.Principal.UserId, $sobytie, $ok)
  if (-not $ok) { $bad++ }
} catch { Say "task ${task}: missing"; $bad++ }

# --- 5. mark for Mekhanik
$m = [ordered]@{ versiya = 1; zadacha = $task; sobytie = $sobytie; skript = $dst; ustanovleno = (Get-Date).ToString('o') }
try { ($m | ConvertTo-Json -Compress) | Set-Content -LiteralPath $metka -Encoding ascii -ErrorAction Stop; Say "mark: $metka" } catch { Say "ERROR mark: $_"; $bad++ }

# --- 6. the old BARS task
Snyat-Prezhnee

# --- 7a. probe of the whole chain: event -> SYSTEM task -> script -> result
function Vremya-Itoga { try { (Get-Item -LiteralPath $itogPut -ErrorAction Stop).LastWriteTimeUtc } catch { [datetime]::MinValue } }
if (-not $bad) {
  Start-Sleep -Seconds 2
  $bylo = Vremya-Itoga
  Say "probe (a): event $sobytie as Mekhanik writes it"
  try { [Diagnostics.EventLog]::WriteEntry('Mekhanik', 'proba ustanovshchika: zadacha sama merit zazhim', [Diagnostics.EventLogEntryType]::Information, $sobytie) }
  catch { Say "  event not written: $_"; $bad++ }
  $t = Get-Date
  while (((Get-Date) - $t).TotalSeconds -lt 100 -and (Vremya-Itoga) -le $bylo) { Start-Sleep -Milliseconds 500 }
  if ((Vremya-Itoga) -gt $bylo) {
    $i = Get-Content -Raw -LiteralPath $itogPut | ConvertFrom-Json
    Say ("  the task answered in {0:N0} s: {1} (step {2}, limit {3} of {4} W)" -f ((Get-Date) - $t).TotalSeconds, $i.itog, $i.shag, $i.predel_do, $i.obychnyy)
  } else { Say '  NO RESULT in 100 s: the task did not run on the event'; $bad++ }
}
# --- 7b. one restart of nvpcf without a clamp
if (-not $BezProby -and -not $bad) {
  Say 'probe (b): one restart of NVIDIA Platform Controllers and Framework (no clamp)'
  & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $dst -Proba | ForEach-Object { Say ('  ' + $_) }
}
if ($bad) { Say "=== DONE WITH ERRORS: $bad ===" } else { Say '=== DONE: Mekhanik heals the GPU clamp itself ===' }
Start-Sleep -Seconds 15
