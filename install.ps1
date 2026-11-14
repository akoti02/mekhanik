# Mekhanik - installer. Separate app, does not need BARS (R-195, 25.10). ASCII only: PS 5.1 reads this file as ANSI.
# No administrator rights. One script:
#   powershell -ExecutionPolicy Bypass -File install.ps1            # build (via the PC build queue) + test + release +
#                                                                   #   first time: register logon task + start; then the
#                                                                   #   program updates itself (see src\obnovlenie.rs)
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Remove    # stop, give everything back, remove task/program/data
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Suho      # DRY RUN on temp folders: prints what would be done,
#                                                                   #   builds/registers/installs NOTHING real
# 13.11 (wave 2a, package "zhizn"): the program is one exe with roles. The logon task runs "mekhanik.exe --zapusk" at
# priority 4 (normal). "--zapusk" keeps the brain (no key) alive and, after a release lands in vypuski\, lets the brain
# switch itself. Admin rights live only in a tiny helper - one admin command: pomoshchnik\ustanovit.ps1.
param([switch]$Remove, [switch]$Suho, [string]$SuhoDir)
$ErrorActionPreference = 'Stop'
$src = $PSScriptRoot

# --- version from Cargo.toml (self-update compares versions, so the folder name is the version)
$cargo = Join-Path $src 'Cargo.toml'
$m = Select-String -Path $cargo -Pattern '^\s*version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $m) { Write-Host 'REFUSED: no version in Cargo.toml'; exit 1 }
$ver = $m.Matches[0].Groups[1].Value

# --- folders: real, or temp ones for the dry run
if ($Suho) {
    if (-not $SuhoDir) { $SuhoDir = Join-Path $env:TEMP ('mekhanik-suho-' + [guid]::NewGuid().ToString('N').Substring(0, 8)) }
    $data = Join-Path $SuhoDir 'mekhanik'
    $prog = Join-Path $SuhoDir 'Programs\Mekhanik'
} else {
    $data = Join-Path $env:LOCALAPPDATA 'mekhanik'
    $prog = Join-Path $env:LOCALAPPDATA 'Programs\Mekhanik'
}
$vypuski = Join-Path $data 'vypuski'
$versii = Join-Path $prog 'versii'
$exe = Join-Path $prog 'mekhanik.exe'           # the stable supervisor stub the task runs
$tek = Join-Path $prog 'tekushchaya.json'       # current-version pointer (read by --zapusk and self-update)

function Sha-Lower([string]$path) { (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLower() }

function Stop-Gently([string]$programExe) {
    $running = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $programExe }
    $running = @($running)
    if (-not $running.Count) { return }
    # 13.11 fix: brain and window run from the same exe - wait for each, finish off only the ones still alive
    $ids = @($running | ForEach-Object { [int]$_.ProcessId })
    New-Item -ItemType Directory -Force $data | Out-Null
    New-Item -ItemType File -Force (Join-Path $data 'stop.flag') | Out-Null
    $t = Get-Date
    while (((Get-Date) - $t).TotalSeconds -lt 15 -and @(Get-Process -Id $ids -ErrorAction SilentlyContinue).Count) {
        Start-Sleep -Milliseconds 250
    }
    $left = @(Get-Process -Id $ids -ErrorAction SilentlyContinue)
    foreach ($p in $left) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        Write-Host "Mekhanik PID $($p.Id) did not exit in 15 s - killed (next start gives back what it held)"
    }
    if (-not $left.Count) { Write-Host "Mekhanik exited by stop.flag (PID $($ids -join ', ')) - everything given back" }
    Remove-Item (Join-Path $data 'stop.flag') -Force -ErrorAction SilentlyContinue
}

# ===== REMOVE =====
if ($Remove) {
    # the task runs the supervisor; ask whoever supervisor launched (versioned exe) and the stub to exit
    foreach ($v in @(Get-ChildItem $versii -Directory -ErrorAction SilentlyContinue)) { Stop-Gently (Join-Path $v.FullName 'mekhanik.exe') }
    Stop-Gently $exe
    Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exe } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    if (Get-ScheduledTask -TaskName 'Mekhanik' -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName 'Mekhanik' -Confirm:$false
        Write-Host 'task Mekhanik removed'
    }
    Remove-Item $prog -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item 'HKCU:\Software\Classes\AppUserModelId\Mekhanik' -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $data -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host 'Mekhanik removed (program, task, notification name, data). Sources stay in' $src
    # the helper with system rights (GPU clamp, audio, firewall) runs as SYSTEM - only an administrator removes it
    if (Test-Path (Join-Path $env:ProgramData 'Mekhanik')) {
        Write-Host 'Helper with system rights stays - remove as administrator:' `
            "powershell -ExecutionPolicy Bypass -File $src\pomoshchnik\ustanovit.ps1 -Snyat"
    }
    exit 0
}

# ===== build the exe (dry run substitutes a fake one) =====
if ($Suho) {
    Write-Host "=== DRY RUN (-Suho): temp folders under $SuhoDir, nothing real is built/installed/registered ==="
    New-Item -ItemType Directory -Force $data, $prog | Out-Null
    $built = Join-Path $SuhoDir 'mekhanik.exe'
    # a stand-in exe so the release/install/pointer/prune steps run for real on temp folders
    Set-Content -LiteralPath $built -Value "fake mekhanik $ver" -Encoding ascii
} else {
    # tests first, then release. Prefer the PC build queue when present (worktrees of wave 2a share it); in the plain
    # repo (C:\dev\mekhanik) there is no queue - then build directly, at low priority as before.
    $queue = @((Join-Path (Split-Path $src -Parent) 'cargo-ochered.ps1'), 'C:\dev\wt\cargo-ochered.ps1') | Where-Object { Test-Path $_ } | Select-Object -First 1
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $src 'target' }
    if ($queue) {
        Write-Host "=== building mekhanik $ver through the PC build queue ==="
        & $queue test -j 4 '--' --test-threads=2
        if ($LASTEXITCODE -ne 0) { Write-Host 'TESTS FAILED - no release'; exit 1 }
        & $queue build --release -j 4
        if ($LASTEXITCODE -ne 0) { Write-Host 'BUILD FAILED'; exit 1 }
    } else {
        Write-Host "=== building mekhanik $ver (direct cargo, low priority) ==="
        Push-Location $src
        $tp = Start-Process cargo -ArgumentList 'test', '-j', '4' -NoNewWindow -PassThru; $null = $tp.Handle
        try { $tp.PriorityClass = 'BelowNormal' } catch {}; $tp.WaitForExit()
        if ($tp.ExitCode -ne 0) { Pop-Location; Write-Host 'TESTS FAILED - no release'; exit 1 }
        $bp = Start-Process cargo -ArgumentList 'build', '--release', '-j', '4' -NoNewWindow -PassThru; $null = $bp.Handle
        try { $bp.PriorityClass = 'BelowNormal' } catch {}; $bp.WaitForExit()
        Pop-Location
        if ($bp.ExitCode -ne 0) { Write-Host 'BUILD FAILED'; exit 1 }
    }
    $built = Join-Path $target 'release\mekhanik.exe'
    if (-not (Test-Path $built)) { Write-Host "BUILD OK but no exe at $built"; exit 1 }
}

# ===== release: vypuski\<ver>\mekhanik.exe + .sha256 (self-update reads this) =====
$relDir = Join-Path $vypuski $ver
New-Item -ItemType Directory -Force $relDir | Out-Null
Copy-Item $built (Join-Path $relDir 'mekhanik.exe') -Force
$sha = Sha-Lower (Join-Path $relDir 'mekhanik.exe')
Set-Content -LiteralPath (Join-Path $relDir 'mekhanik.exe.sha256') -Value "$sha *mekhanik.exe" -Encoding ascii
Write-Host "release: $relDir (sha $($sha.Substring(0,12)))"

# ===== who runs now (13.11 fix: the stub was copied over a running exe and the copy failed) =====
# New model already running (the supervisor "--zapusk" from the stable path): only the release is needed - the program
# picks it up from vypuski\ and switches itself (src\obnovlenie.rs). Old model (0.5.x, no key) or nothing: stop it
# gently (stop.flag - it gives back everything it held) before the stub is replaced.
if (-not $Suho) {
    $runStub = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exe })
    $supervisor = @($runStub | Where-Object { $_.CommandLine -match '--zapusk' })
    if ($supervisor.Count) {
        Write-Host "supervisor runs (PID $($supervisor[0].ProcessId)) - release is in vypuski; the program switches itself within a minute"
        exit 0
    }
    # 13.11 fix: the 0.5 task restarts a stopped brain after a minute (RestartOnFailure) and locks the stub again -
    # hold (disable) the task while the stub is replaced; it is registered/enabled again below
    if (Get-ScheduledTask -TaskName 'Mekhanik' -ErrorAction SilentlyContinue) {
        try { Disable-ScheduledTask -TaskName 'Mekhanik' -ErrorAction Stop | Out-Null; Write-Host 'task Mekhanik held (disabled) for the switch' } catch {}
    }
    if ($runStub.Count) { Stop-Gently $exe }
    foreach ($v in @(Get-ChildItem $versii -Directory -ErrorAction SilentlyContinue)) { Stop-Gently (Join-Path $v.FullName 'mekhanik.exe') }
}

# ===== install this version: versii\<ver>\ + current pointer + supervisor stub =====
$verDir = Join-Path $versii $ver
New-Item -ItemType Directory -Force $verDir | Out-Null
Copy-Item (Join-Path $relDir 'mekhanik.exe') (Join-Path $verDir 'mekhanik.exe') -Force
if ((Sha-Lower (Join-Path $verDir 'mekhanik.exe')) -ne $sha) { Write-Host 'ERROR: exe differs after copy into versii'; exit 1 }
$prevVer = $null
if (Test-Path $tek) { try { $prevVer = (Get-Content -Raw $tek | ConvertFrom-Json).versiya } catch {} }
$pointer = [ordered]@{ versiya = $ver }
if ($prevVer -and $prevVer -ne $ver) { $pointer.prezhnyaya = $prevVer }
($pointer | ConvertTo-Json -Compress) | Set-Content -LiteralPath $tek -Encoding ascii
Copy-Item (Join-Path $verDir 'mekhanik.exe') $exe -Force   # stable path the logon task runs (--zapusk)
Write-Host "installed: $verDir ; current -> $ver ; supervisor stub -> $exe"

# keep current + two previous versions (self-update prunes too; here on install)
$all = @(Get-ChildItem $versii -Directory -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
$keep = @($ver)
$keep += @($all | Where-Object { $_ -ne $ver } | Sort-Object { [version]$_ } -ErrorAction SilentlyContinue | Select-Object -Last 2)
foreach ($v in $all) { if ($keep -notcontains $v) { Remove-Item (Join-Path $versii $v) -Recurse -Force; Write-Host "pruned old version $v" } }

# ===== logon task: SYSTEM-free, priority 4 (normal), action = mekhanik.exe --zapusk =====
$taskXml = @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Author>Mekhanik</Author><Description>Mekhanik: start the supervisor at logon (it keeps the brain alive and lets it self-update)</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>$env:USERDOMAIN\$env:USERNAME</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>$env:USERDOMAIN\$env:USERNAME</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
    <!-- restarts "--zapusk" if it dies unexpectedly (non-zero exit). On give-up (5 crashes/hour) "--zapusk" exits 0 on
         purpose, so this does NOT resurrect a broken version - the brain stays down until logon or a manual start. -->
    <RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author"><Exec><Command>$exe</Command><Arguments>--zapusk</Arguments></Exec></Actions>
</Task>
"@

if ($Suho) {
    Write-Host '--- would register logon task "Mekhanik" (priority 4, action: mekhanik.exe --zapusk) ; NOT registered (dry run)'
    # verify the release/install chain really happened on temp folders
    $bad = 0
    if (-not (Test-Path (Join-Path $relDir 'mekhanik.exe.sha256'))) { Write-Host 'BAD: no .sha256 in release'; $bad++ }
    if ((Sha-Lower (Join-Path $verDir 'mekhanik.exe')) -ne $sha) { Write-Host 'BAD: installed exe sha mismatch'; $bad++ }
    $tj = Get-Content -Raw $tek | ConvertFrom-Json
    if ($tj.versiya -ne $ver) { Write-Host "BAD: pointer version $($tj.versiya) != $ver"; $bad++ }
    if ([Xml]$taskXml) { Write-Host 'task XML parses ok' }
    Write-Host "--- would run: schtasks /run /tn Mekhanik ; NOT started (dry run)"
    if (-not $SuhoDir -or $SuhoDir -like (Join-Path $env:TEMP '*')) { Remove-Item $SuhoDir -Recurse -Force -ErrorAction SilentlyContinue; Write-Host "temp folders removed: $SuhoDir" }
    if ($bad) { Write-Host "=== DRY RUN DONE WITH ERRORS: $bad ==="; exit 1 } else { Write-Host '=== DRY RUN OK: release + install + pointer + prune + task XML all good ==='; exit 0 }
}

$task = Get-ScheduledTask -TaskName 'Mekhanik' -ErrorAction SilentlyContinue
# 13.11 fix: the task of 0.5.x (no "--zapusk", default priority 7 = below normal) is replaced by the new one; a task
# that already runs the supervisor is left as is (the program updates itself)
$staraya = $task -and (-not (@($task.Actions | Where-Object { $_.Arguments -match '--zapusk' }).Count) -or $task.Settings.Priority -ne 4)
if (-not $task -or $staraya) {
    try { Register-ScheduledTask -TaskName 'Mekhanik' -Xml $taskXml -Force -ErrorAction Stop | Out-Null; Write-Host "task Mekhanik $(if ($staraya) { 're-registered (was 0.5: no --zapusk or priority not 4)' } else { 'registered' }) (at logon, priority 4, --zapusk)" }
    catch { Write-Host "TASK NOT REGISTERED: $($_.Exception.Message)"; exit 1 }
} else { Write-Host 'task Mekhanik already runs the supervisor - left as is (the program updates itself)' }
try { Enable-ScheduledTask -TaskName 'Mekhanik' -ErrorAction Stop | Out-Null } catch {}

Stop-Gently $exe
schtasks /run /tn Mekhanik | Out-Null
Start-Sleep -Seconds 5
$new = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like (Join-Path $prog '*mekhanik.exe') }
if ($new) { Write-Host "Mekhanik running: $($new.ProcessId -join ', ')" } else { Write-Host 'MEKHANIK NOT RUNNING'; exit 1 }
Write-Host "=== DONE: mekhanik $ver installed; it updates itself from vypuski\ from now on ==="
