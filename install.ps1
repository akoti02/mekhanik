# Mekhanik - installer. Separate app, does not need BARS (R-195, 25.10). ASCII only: PS 5.1 reads this file as ANSI.
#   powershell -ExecutionPolicy Bypass -File C:\dev\mekhanik\install.ps1           # build + install/update + start
#   powershell -ExecutionPolicy Bypass -File C:\dev\mekhanik\install.ps1 -Remove   # stop, give everything back, remove
# Install: 1. release build (low priority, -j 4); 2. ask the running app to exit (stop.flag: it releases every frozen
# process and gives back every priority it lowered), kill by PID only if it did not exit in 15 s; 3. copy mekhanik.exe to
# %LOCALAPPDATA%\Programs\Mekhanik; 4. make sure the logon task "Mekhanik" exists (no admin needed); 5. start it through
# the task - never as a child of the calling shell (a process started from a terminal session dies with it).
param([switch]$Remove)
$ErrorActionPreference = 'Stop'
$src = $PSScriptRoot
$dst = Join-Path $env:LOCALAPPDATA 'Programs\Mekhanik'
$dir = Join-Path $env:LOCALAPPDATA 'mekhanik'
$exe = Join-Path $dst 'mekhanik.exe'

function Stop-Gently {
    $running = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exe }
    if (-not $running) { return }
    New-Item -ItemType Directory -Force $dir | Out-Null
    New-Item -ItemType File -Force (Join-Path $dir 'stop.flag') | Out-Null
    $t = Get-Date
    while (((Get-Date) - $t).TotalSeconds -lt 15 -and (Get-Process -Id $running.ProcessId -ErrorAction SilentlyContinue)) {
        Start-Sleep -Milliseconds 250
    }
    if (Get-Process -Id $running.ProcessId -ErrorAction SilentlyContinue) {
        Stop-Process -Id $running.ProcessId -Force
        Write-Host "Mekhanik did not exit in 15 s - killed PID $($running.ProcessId) (next start gives back what it held)"
    } else { Write-Host "Mekhanik exited by stop.flag (PID $($running.ProcessId)) - everything given back" }
    Remove-Item (Join-Path $dir 'stop.flag') -Force -ErrorAction SilentlyContinue
}

if ($Remove) {
    Stop-Gently
    if (Get-ScheduledTask -TaskName 'Mekhanik' -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName 'Mekhanik' -Confirm:$false
        Write-Host 'task Mekhanik removed'
    }
    Remove-Item $dst -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item 'HKCU:\Software\Classes\AppUserModelId\Mekhanik' -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host 'Mekhanik removed (program, task, notification name, data). Sources stay in' $src
    exit 0
}

$prev = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
Push-Location $src
$p = Start-Process cargo -ArgumentList 'build', '--release', '-j', '4' -NoNewWindow -PassThru
# without a held handle PS 5.1 loses the exit code (ExitCode stays empty)
$null = $p.Handle
try { $p.PriorityClass = 'BelowNormal' } catch {}
$p.WaitForExit()
Pop-Location
$ErrorActionPreference = $prev
if ($p.ExitCode -ne 0) { Write-Host 'BUILD FAILED'; exit 1 }

Stop-Gently
New-Item -ItemType Directory -Force $dst, $dir | Out-Null
Copy-Item (Join-Path $src 'target\release\mekhanik.exe') $exe -Force

$task = Get-ScheduledTask -TaskName 'Mekhanik' -ErrorAction SilentlyContinue
if (-not $task) {
    $act = New-ScheduledTaskAction -Execute $exe
    $trg = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
    $set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) `
        -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    try {
        Register-ScheduledTask -TaskName 'Mekhanik' -Action $act -Trigger $trg -Settings $set -Force -ErrorAction Stop | Out-Null
        Write-Host 'task Mekhanik registered (at logon)'
    } catch { Write-Host "TASK NOT REGISTERED: $($_.Exception.Message)" }
}
schtasks /run /tn Mekhanik | Out-Null
Start-Sleep -Seconds 4
$new = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exe }
if ($new) { Write-Host "Mekhanik running: PID $($new.ProcessId), parent $($new.ParentProcessId)" } else { Write-Host 'MEKHANIK NOT RUNNING'; exit 1 }
