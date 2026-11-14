# Mekhanik - INSTALL THE HELPER WITH SYSTEM RIGHTS (13.11.2026, wave 2a package "zhizn"). ASCII only.
# ONE admin command, idempotent (asks Windows itself for rights). Mekhanik itself runs WITHOUT admin; only this tiny
# helper has system rights, and it does two small, fixed things - it does NOT update itself (only this command does):
#   1) task Mekhanik-Videokarta on event 4201: GPU power clamp healing (zazhim-lechit.ps1) - already stood since 13.11,
#      this command does not break it;
#   2) task Mekhanik-Zvuk on event 4202: restart the audio service on a real audiodg leak, ONLY when nothing plays
#      (zvuk-lechit.ps1 checks by itself + rate limit).
#   (14.11: the firewall rule is gone - it matched only the launcher stub by path, never the brain in versii\<ver>\;
#    an old rule from an earlier install is removed by this command.)
# Each event is only a reason; each script measures the need itself and limits its own rate.
#
#   powershell -ExecutionPolicy Bypass -File pomoshchnik\ustanovit.ps1            # install / update the helper (admin)
#   powershell -ExecutionPolicy Bypass -File pomoshchnik\ustanovit.ps1 -Snyat     # remove the helper (admin)
#   powershell -ExecutionPolicy Bypass -File pomoshchnik\ustanovit.ps1 -Suho      # DRY RUN: no admin, registers
#                                                                                 #   NOTHING, prints what would be done
#   -BezProby : skip the extra live nvpcf restart probe (b)
param([switch]$BezProby, [switch]$Snyat, [switch]$Suho, [string]$UserLocalAppData = '', [string]$SuhoDir = '')
$ErrorActionPreference = 'Continue'
$dir = Join-Path $env:ProgramData 'Mekhanik'
$log = Join-Path $dir 'ustanovka.log'
$zadachaGpu = 'Mekhanik-Videokarta'
$zadachaZvuk = 'Mekhanik-Zvuk'
$sobGpu = 4201
$sobZvuk = 4202
$pravilaSeti = @('Mekhanik-Oflayn-Out', 'Mekhanik-Oflayn-In')
$srcGpu = Join-Path $PSScriptRoot 'zazhim-lechit.ps1'
$srcZvuk = Join-Path $PSScriptRoot 'zvuk-lechit.ps1'
$dstGpu = Join-Path $dir 'zazhim-lechit.ps1'
$dstZvuk = Join-Path $dir 'zvuk-lechit.ps1'
$metka = Join-Path $dir 'videokarta.json'
$metkaZvuk = Join-Path $dir 'zvuk.json'
$itogGpu = Join-Path $dir 'zazhim_itog.json'
# the user profile, captured before elevation (admin profile is a different one)
if (-not $UserLocalAppData) { $UserLocalAppData = $env:LOCALAPPDATA }

function Parses([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return $false }
  $e = $null
  [void][System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$null, [ref]$e)
  $bytes = [IO.File]::ReadAllBytes($path)
  return (-not ($e -and $e.Count)) -and (-not (@($bytes | Where-Object { $_ -gt 127 }).Count))
}

# XML of a SYSTEM task triggered by an Application-log event of source Mekhanik
function Task-Xml([string]$imya, [int]$sobytie, [string]$skript) {
  $sub = "&lt;QueryList&gt;&lt;Query Id=`"0`" Path=`"Application`"&gt;&lt;Select Path=`"Application`"&gt;*[System[Provider[@Name='Mekhanik'] and (EventID=$sobytie)]]&lt;/Select&gt;&lt;/Query&gt;&lt;/QueryList&gt;"
@"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Author>Mekhanik</Author><Description>Mekhanik helper: $imya on event $sobytie of Mekhanik (Application) - $skript</Description></RegistrationInfo>
  <Triggers><EventTrigger><Enabled>true</Enabled><Subscription>$sub</Subscription></EventTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>S-1-5-18</UserId><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
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
  <Actions Context="Author"><Exec><Command>powershell.exe</Command><Arguments>-NonInteractive -NoProfile -ExecutionPolicy Bypass -File "$skript"</Arguments></Exec></Actions>
</Task>
"@
}

# ===== DRY RUN: no admin, change nothing =====
if ($Suho) {
  if (-not $SuhoDir) { $SuhoDir = Join-Path $env:TEMP ('mekhanik-pom-suho-' + [guid]::NewGuid().ToString('N').Substring(0, 8)) }
  Write-Output "=== DRY RUN (-Suho): no admin, registers NOTHING. Temp folder $SuhoDir ==="
  $bad = 0
  foreach ($s in @($srcGpu, $srcZvuk)) {
    if (Parses $s) { Write-Output "ok   script parses and is ASCII: $(Split-Path $s -Leaf)" }
    else { Write-Output "BAD  script missing / parse error / non-ASCII: $s"; $bad++ }
  }
  foreach ($t in @(@{n = $zadachaGpu; e = $sobGpu; s = $dstGpu }, @{n = $zadachaZvuk; e = $sobZvuk; s = $dstZvuk })) {
    try { [void][Xml](Task-Xml $t.n $t.e $t.s); Write-Output "ok   would register task $($t.n) on event $($t.e) (SYSTEM) -> $($t.s)" }
    catch { Write-Output "BAD  task XML for $($t.n) does not parse: $_"; $bad++ }
  }
  Write-Output "---  would run GPU clamp probe (a): write event $sobGpu, expect the task to answer 'ne_nuzhno' (no clamp)"
  Write-Output "---  NO firewall rule (14.11): the brain runs from versii\<version>\ - a rule by path would miss it; offline is held by the code itself; old rules $($pravilaSeti -join ', ') would be removed if present"
  Write-Output "---  would copy scripts into $dir (owner Administrators; SYSTEM+Admins write, Users read)"
  # prove the dry run does the harmless file steps on a temp folder
  New-Item -ItemType Directory -Force $SuhoDir | Out-Null
  Copy-Item $srcGpu (Join-Path $SuhoDir 'zazhim-lechit.ps1') -Force
  Copy-Item $srcZvuk (Join-Path $SuhoDir 'zvuk-lechit.ps1') -Force
  if ((Get-FileHash $srcGpu -Algorithm SHA256).Hash -ne (Get-FileHash (Join-Path $SuhoDir 'zazhim-lechit.ps1') -Algorithm SHA256).Hash) { Write-Output 'BAD: GPU script differs after copy'; $bad++ }
  if ((Get-FileHash $srcZvuk -Algorithm SHA256).Hash -ne (Get-FileHash (Join-Path $SuhoDir 'zvuk-lechit.ps1') -Algorithm SHA256).Hash) { Write-Output 'BAD: audio script differs after copy'; $bad++ }
  Remove-Item $SuhoDir -Recurse -Force -ErrorAction SilentlyContinue
  if ($bad) { Write-Output "=== DRY RUN DONE WITH ERRORS: $bad ==="; exit 1 } else { Write-Output '=== DRY RUN OK ==='; exit 0 }
}

# ===== real run needs admin; elevate (Windows asks "allow?" - that is his permission) =====
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"", '-UserLocalAppData', "`"$UserLocalAppData`"")
  if ($BezProby) { $a += '-BezProby' }
  if ($Snyat) { $a += '-Snyat' }
  try { Start-Process powershell.exe -Verb RunAs -ArgumentList $a -ErrorAction Stop; Write-Output 'asked for administrator rights - see the Windows window' }
  catch { Write-Output 'ADMINISTRATOR NEEDED: permission not given' }
  exit 0
}

function Say([string]$m) {
  $line = ('{0}  {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $m)
  Write-Output $line
  if (Test-Path -LiteralPath $dir) {
    # 14.11 (audit): rotate like the healer logs (512 KB)
    try { if ((Get-Item -LiteralPath $log -ErrorAction Stop).Length -gt 512KB) { Move-Item -LiteralPath $log -Destination "$log.old" -Force } } catch {}
    try { Add-Content -Path $log -Value $line -Encoding ascii } catch {}
  }
}

# the job before 13.11 lived in BARS (task BARS-Videokarta) - remove if present
function Snyat-Prezhnee {
  try { Unregister-ScheduledTask -TaskName 'BARS-Videokarta' -Confirm:$false -ErrorAction Stop; Say 'old task BARS-Videokarta removed' } catch {}
  foreach ($f in 'zazhim-lechit.ps1', 'zazhim_pamyat.txt', 'zazhim.log') {
    $p = Join-Path $env:ProgramData "BARS\$f"
    if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue; Say "old $p removed" }
  }
}

if ($Snyat) {
  Say '=== removing the Mekhanik helper (system rights) ==='
  foreach ($t in @($zadachaGpu, $zadachaZvuk)) {
    try { Unregister-ScheduledTask -TaskName $t -Confirm:$false -ErrorAction Stop; Say "task $t removed" } catch { Say "task ${t}: none" }
  }
  foreach ($r in $pravilaSeti) {
    try { if (Get-NetFirewallRule -Name $r -ErrorAction SilentlyContinue) { Remove-NetFirewallRule -Name $r -ErrorAction Stop; Say "firewall rule $r removed" } else { Say "firewall rule ${r}: none" } } catch { Say "firewall ${r}: $_" }
  }
  Snyat-Prezhnee
  try { if ([Diagnostics.EventLog]::SourceExists('Mekhanik')) { [Diagnostics.EventLog]::DeleteEventSource('Mekhanik'); Say 'event source Mekhanik removed' } } catch { Say "event source: $_" }
  Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
  Write-Output "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  folder $dir removed"
  Write-Output '=== DONE: helper removed (Mekhanik itself keeps running; install.ps1 -Remove removes it) ==='
  Start-Sleep -Seconds 10
  exit 0
}

# a broken script is not put in place: the task would run it with system rights
if (-not (Parses $srcGpu)) { Write-Output "REFUSED: $srcGpu missing / parse error / non-ASCII"; Start-Sleep 15; exit 1 }
if (-not (Parses $srcZvuk)) { Write-Output "REFUSED: $srcZvuk missing / parse error / non-ASCII"; Start-Sleep 15; exit 1 }

$bad = 0
Say '=== installing the Mekhanik helper (system rights) ==='

# --- 1. folder: owner Administrators, SYSTEM+Admins full, Users read
New-Item -ItemType Directory -Force -Path $dir | Out-Null
& icacls.exe $dir /setowner '*S-1-5-32-544' /T /C /Q 2>&1 | Out-Null
& icacls.exe $dir /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX' /C /Q 2>&1 | Out-Null
if (Get-ChildItem -LiteralPath $dir -Force -ErrorAction SilentlyContinue) { & icacls.exe "$dir\*" /reset /T /C /Q 2>&1 | Out-Null }
$vladelec = (Get-Acl -LiteralPath $dir).GetOwner([Security.Principal.SecurityIdentifier]).Value
if ($vladelec -ne 'S-1-5-32-544') { Say "ERROR folder owner: $vladelec"; $bad++ } else { Say "folder ${dir}: owner Administrators" }
# 14.11 (audit): the folder holds scripts that run as SYSTEM - check the rights for real, not only the owner: inheritance
# off and no write/modify for Users (S-1-5-32-545), Authenticated Users (S-1-5-11) or Everyone (S-1-1-0)
$acl = Get-Acl -LiteralPath $dir
$pisat = [Security.AccessControl.FileSystemRights]'WriteData, AppendData, WriteExtendedAttributes, DeleteSubdirectoriesAndFiles, WriteAttributes, Delete, ChangePermissions, TakeOwnership'
$chuzhie = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]) | Where-Object {
  $_.AccessControlType -eq 'Allow' -and @('S-1-5-32-545', 'S-1-5-11', 'S-1-1-0') -contains $_.IdentityReference.Value -and
  ($_.FileSystemRights -band $pisat) -ne 0 })
if (-not $acl.AreAccessRulesProtected) { Say "ERROR folder ${dir}: rights still inherited from ProgramData"; $bad++ }
elseif ($chuzhie.Count) { Say "ERROR folder ${dir}: non-admins can write ($($chuzhie.Count) rules)"; $bad++ }
else { Say "folder ${dir}: only SYSTEM and Administrators can write" }

# --- 2. scripts
foreach ($pair in @(@{s = $srcGpu; d = $dstGpu }, @{s = $srcZvuk; d = $dstZvuk })) {
  Copy-Item $pair.s $pair.d -Force
  if ((Get-FileHash $pair.s -Algorithm SHA256).Hash -eq (Get-FileHash $pair.d -Algorithm SHA256).Hash) { Say "script put: $($pair.d)" } else { Say "ERROR: $($pair.d) differs after copy"; $bad++ }
}

# --- 3. event source
try {
  if (-not [Diagnostics.EventLog]::SourceExists('Mekhanik')) { New-EventLog -LogName Application -Source 'Mekhanik' -ErrorAction Stop; Say 'event source Mekhanik registered' }
  else { Say 'event source Mekhanik: exists' }
} catch { Say "event source not registered (events still work): $_" }

# --- 4. tasks (idempotent: -Force re-registers the same definition)
foreach ($t in @(@{n = $zadachaGpu; e = $sobGpu; s = $dstGpu }, @{n = $zadachaZvuk; e = $sobZvuk; s = $dstZvuk })) {
  try { Register-ScheduledTask -TaskName $t.n -Xml (Task-Xml $t.n $t.e $t.s) -Force -ErrorAction Stop | Out-Null; Say "task $($t.n): registered (event $($t.e), SYSTEM)" }
  catch { Say "ERROR task $($t.n): $_"; $bad++ }
  try {
    $s = Get-ScheduledTask -TaskName $t.n -ErrorAction Stop
    $sub = [string]($s.Triggers | Select-Object -First 1).Subscription
    $ok = ($s.Principal.UserId -match 'SYSTEM') -and ($sub -match "EventID=$($t.e)") -and (-not $s.Settings.DisallowStartIfOnBatteries)
    Say ("task $($t.n): as {0}, event {1}, also on batteries - {2}" -f $s.Principal.UserId, $t.e, $ok)
    if (-not $ok) { $bad++ }
  } catch { Say "task $($t.n): missing"; $bad++ }
}

# --- 5. NO firewall rule (14.11): the brain runs from versii\<version>\mekhanik.exe and the path changes with every
# self-update - a rule by program path matched only the launcher stub, a false lock. Offline is held by the code itself
# (no network APIs; it checks its own connections). Rules from an earlier install are removed.
foreach ($r in $pravilaSeti) {
  try { if (Get-NetFirewallRule -Name $r -ErrorAction SilentlyContinue) { Remove-NetFirewallRule -Name $r -ErrorAction Stop; Say "old firewall rule $r removed" } } catch { Say "firewall ${r}: $_" }
}

# --- 6. marks for Mekhanik
try { ([ordered]@{ versiya = 1; zadacha = $zadachaGpu; sobytie = $sobGpu; skript = $dstGpu; ustanovleno = (Get-Date).ToString('o') } | ConvertTo-Json -Compress) | Set-Content -LiteralPath $metka -Encoding ascii -ErrorAction Stop; Say "mark: $metka" } catch { Say "ERROR mark: $_"; $bad++ }
try { ([ordered]@{ versiya = 1; zadacha = $zadachaZvuk; sobytie = $sobZvuk; skript = $dstZvuk; ustanovleno = (Get-Date).ToString('o') } | ConvertTo-Json -Compress) | Set-Content -LiteralPath $metkaZvuk -Encoding ascii -ErrorAction Stop; Say "mark: $metkaZvuk" } catch { Say "ERROR mark: $_"; $bad++ }

# --- 7. the old BARS task
Snyat-Prezhnee

# --- 8. probe the GPU chain: event -> SYSTEM task -> script -> result (no clamp -> 'ne_nuzhno')
function Vremya-Itoga { try { (Get-Item -LiteralPath $itogGpu -ErrorAction Stop).LastWriteTimeUtc } catch { [datetime]::MinValue } }
if (-not $bad) {
  Start-Sleep -Seconds 2
  $bylo = Vremya-Itoga
  Say "probe (a): event $sobGpu as Mekhanik writes it"
  try { [Diagnostics.EventLog]::WriteEntry('Mekhanik', 'proba ustanovshchika: zadacha sama merit zazhim', [Diagnostics.EventLogEntryType]::Information, $sobGpu) }
  catch { Say "  event not written: $_"; $bad++ }
  $t = Get-Date
  while (((Get-Date) - $t).TotalSeconds -lt 100 -and (Vremya-Itoga) -le $bylo) { Start-Sleep -Milliseconds 500 }
  if ((Vremya-Itoga) -gt $bylo) {
    $i = Get-Content -Raw -LiteralPath $itogGpu | ConvertFrom-Json
    Say ("  the task answered in {0:N0} s: {1} (step {2})" -f ((Get-Date) - $t).TotalSeconds, $i.itog, $i.shag)
  } else { Say '  NO RESULT in 100 s: the task did not run on the event'; $bad++ }
}
# --- 8b. one live nvpcf restart without a clamp
if (-not $BezProby -and -not $bad) {
  Say 'probe (b): one restart of NVIDIA Platform Controllers and Framework (no clamp)'
  & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $dstGpu -Proba | ForEach-Object { Say ('  ' + $_) }
}
if ($bad) { Say "=== DONE WITH ERRORS: $bad ===" } else { Say '=== DONE: helper installed (GPU clamp 4201, audio 4202) ===' }
Start-Sleep -Seconds 15
