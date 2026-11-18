#Requires -Version 7.0
<#
.SYNOPSIS
    Runs the Windows service control manager smoke test.

.DESCRIPTION
    Builds a temporary service, runs the install, status, stop, start,
    restart, and uninstall commands, and checks the service states, the
    process, the busy and pid files, and the log.

    The script needs an administrator PowerShell, Windows NT or later, and
    the release binary. It removes the service and the temporary directory
    at the end.

.EXAMPLE
    pwsh -NoProfile -File tests/windows-smoke.ps1
#>
[CmdletBinding()]
param(
    [string]$Binary,
    [switch]$KeepTemp
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
if (-not $Binary) {
    $Binary = Join-Path $repo 'target/release/rbinkd.exe'
}
if (-not (Test-Path -LiteralPath $Binary)) {
    throw "The rbinkd binary is missing: $Binary. Run cargo build --release -p rbinkd."
}
$Binary = (Resolve-Path -LiteralPath $Binary).Path

$tempRoot = if ($env:RUNNER_TEMP) {
    $env:RUNNER_TEMP
} else {
    [System.IO.Path]::GetTempPath()
}
$root = Join-Path $tempRoot ('binkd-smoke-' + [guid]::NewGuid().ToString('N'))
$serviceName = 'BinkdSmoke' + [guid]::NewGuid().ToString('N').Substring(0, 12)
$log = Join-Path $root 'binkd.log'
$pidFile = Join-Path $root 'binkd.pid'
$config = Join-Path $root 'binkd.conf'
$outbound = Join-Path $root 'outbound'
$inbound = Join-Path $root 'inbound'

function Write-Step {
    param([string]$Text)
    Write-Host "=== $Text ==="
}

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Contains {
    param([string]$Text, [string]$Expected)
    if (-not $Text.Contains($Expected)) {
        throw "Expected '$Expected' in the output:`n$Text"
    }
}

function Invoke-Rbinkd {
    param([string[]]$Arguments)
    $output = & $Binary @Arguments 2>&1
    $code = $LASTEXITCODE
    $text = ($output | Out-String).Trim()
    if ($code -ne 0) {
        throw "rbinkd $($Arguments -join ' ') exited with $code`n$text"
    }
    return $text
}

function Invoke-Sc {
    param([string[]]$Arguments)
    $output = & sc.exe @Arguments 2>&1
    $code = $LASTEXITCODE
    $text = ($output | Out-String).Trim()
    if ($code -ne 0) {
        throw "sc.exe $($Arguments -join ' ') exited with $code`n$text"
    }
    return $text
}

function Wait-ServiceState {
    param(
        [string]$Name,
        [string]$State,
        [int]$TimeoutSeconds = 30
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $service = Get-Service -Name $Name -ErrorAction SilentlyContinue
        if ($State -eq 'Absent') {
            if ($null -eq $service) {
                return
            }
        } elseif ($null -ne $service -and $service.Status.ToString() -eq $State) {
            return
        }
        Start-Sleep -Milliseconds 250
    }
    throw "The service $Name did not reach the state $State."
}

function Assert-ServiceState {
    param([string]$Name, [string]$State)
    $service = Get-Service -Name $Name -ErrorAction SilentlyContinue
    if ($State -eq 'Absent') {
        Assert-True -Condition ($null -eq $service) `
            -Message "The service $Name still exists."
    } else {
        Assert-True -Condition ($null -ne $service) `
            -Message "The service $Name does not exist."
        Assert-True -Condition ($service.Status.ToString() -eq $State) `
            -Message "The service $Name is $($service.Status), not $State."
    }
}

function Wait-NoProcess {
    param([int]$TimeoutSeconds = 30)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($null -eq (Get-Process -Name rbinkd -ErrorAction SilentlyContinue)) {
            return
        }
        Start-Sleep -Milliseconds 250
    }
    throw 'An rbinkd process is still running.'
}

function Assert-NoBusy {
    $busy = @(Get-ChildItem -LiteralPath $outbound, $inbound -Filter '*.bsy' `
            -Recurse -ErrorAction SilentlyContinue)
    $busyNames = @($busy | ForEach-Object { $_.FullName })
    Assert-True -Condition ($busy.Count -eq 0) `
        -Message "Busy files remain: $($busyNames -join ', ')"
    Assert-True -Condition (-not (Test-Path -LiteralPath $pidFile)) `
        -Message "The pid file remains: $pidFile"
}

function Read-Log {
    Assert-True -Condition (Test-Path -LiteralPath $log) `
        -Message "The log file is missing: $log"
    return Get-Content -LiteralPath $log -Raw
}

function Assert-LogContains {
    param([string]$Text, [string]$Expected)
    if (-not $Text.Contains($Expected)) {
        throw "Expected '$Expected' in the log."
    }
}

function Assert-LogNotContains {
    param([string]$Text, [string]$Unexpected)
    if ($Text.Contains($Unexpected)) {
        throw "The log holds '$Unexpected'."
    }
}

New-Item -ItemType Directory -Path $outbound, $inbound -Force | Out-Null
# The config format treats a backslash as an escape, so the Windows paths use
# forward slashes, the form of the C sample configs.
function ConvertTo-ConfigPath {
    param([string]$Path)
    return $Path.Replace('\', '/')
}

$configText = @"
domain fidonet $(ConvertTo-ConfigPath $outbound) 2
address 2:5047/100@fidonet
sysname "Binkd smoke test"
sysop "CI"
location "CI"
nodeinfo "115200,TCP,BINKP"
inbound $(ConvertTo-ConfigPath $inbound)
log $(ConvertTo-ConfigPath $log)
loglevel 8
conlog 8
pid-file $(ConvertTo-ConfigPath $pidFile)
"@
Set-Content -LiteralPath $config -Value $configText -Encoding Ascii

$installed = $false
Push-Location $root
try {
    Write-Step 'Install'
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'install', '-S', $serviceName, '-c', 'binkd.conf'
    )
    $installed = $true
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' installed and started successfully."
    Wait-ServiceState -Name $serviceName -State 'Running'
    Assert-ServiceState -Name $serviceName -State 'Running'

    $service = Get-CimInstance -ClassName Win32_Service `
        -Filter "Name='$serviceName'"
    Assert-True -Condition ($null -ne $service) `
        -Message 'The service is not in the service control manager.'
    Assert-True -Condition ($service.PathName -like '*rbinkd.exe*') `
        -Message "The service binary path is not rbinkd.exe: $($service.PathName)"
    $parameters = Get-ItemProperty `
        -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName\Parameters"
    Assert-True -Condition ($parameters.path -eq $root) `
        -Message "The registry working directory is $($parameters.path), not $root."

    Write-Step 'Status'
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'status', '-S', $serviceName
    )
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' is installed and is running."

    Write-Step 'Stop'
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'stop', '-S', $serviceName
    )
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' stopped successfully."
    Wait-ServiceState -Name $serviceName -State 'Stopped'
    Assert-ServiceState -Name $serviceName -State 'Stopped'
    Wait-NoProcess
    Assert-NoBusy
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'status', '-S', $serviceName
    )
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' is installed and is stopped."

    Write-Step 'Start'
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'start', '-S', $serviceName
    )
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' started successfully."
    Wait-ServiceState -Name $serviceName -State 'Running'
    Assert-ServiceState -Name $serviceName -State 'Running'

    Write-Step 'Stop through sc.exe'
    $output = Invoke-Sc -Arguments @('stop', $serviceName)
    Wait-ServiceState -Name $serviceName -State 'Stopped'
    Assert-ServiceState -Name $serviceName -State 'Stopped'
    Wait-NoProcess
    Assert-NoBusy

    Write-Step 'Start through sc.exe'
    $output = Invoke-Sc -Arguments @('start', $serviceName)
    Wait-ServiceState -Name $serviceName -State 'Running'
    Assert-ServiceState -Name $serviceName -State 'Running'

    Write-Step 'Restart'
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'restart', '-S', $serviceName
    )
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' restarted successfully."
    Wait-ServiceState -Name $serviceName -State 'Running'
    Assert-ServiceState -Name $serviceName -State 'Running'

    Write-Step 'Uninstall'
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'uninstall', '-S', $serviceName
    )
    $installed = $false
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' uninstalled successfully."
    Wait-ServiceState -Name $serviceName -State 'Absent'
    Assert-ServiceState -Name $serviceName -State 'Absent'
    $output = Invoke-Rbinkd -Arguments @(
        '-t', 'status', '-S', $serviceName
    )
    Assert-Contains -Text $output `
        -Expected "Service '$serviceName' is not installed."
    Wait-NoProcess
    Assert-NoBusy

    Write-Step 'Log output'
    $logText = Read-Log
    Assert-LogContains -Text $logText `
        -Expected "BEGIN service '$serviceName'"
    Assert-LogNotContains -Text $logText -Unexpected 'BEGIN standalone'
    Assert-LogNotContains -Text $logText -Unexpected 'FATAL'
    Assert-LogNotContains -Text $logText -Unexpected 'error in configuration'
    $beginCount = ([regex]::Matches(
            $logText,
            [regex]::Escape("BEGIN service '$serviceName'")
        )).Count
    Assert-True -Condition ($beginCount -ge 4) `
        -Message "The log holds $beginCount BEGIN service lines, not 4."
    $exitCount = ([regex]::Matches(
            $logText,
            [regex]::Escape('exitfunc()')
        )).Count
    Assert-True -Condition ($exitCount -ge 4) `
        -Message "The log holds $exitCount exitfunc() lines, not 4."

    Write-Host 'The Windows service smoke test passed.'
}
catch {
    Write-Host "The Windows service smoke test failed: $_"
    if (Test-Path -LiteralPath $log) {
        Write-Host '--- binkd.log ---'
        Get-Content -LiteralPath $log -ErrorAction SilentlyContinue |
            ForEach-Object { Write-Host $_ }
        Write-Host '--- end of binkd.log ---'
    }
    throw
}
finally {
    if ($installed -or (Get-Service -Name $serviceName -ErrorAction SilentlyContinue)) {
        & $Binary -t stop -S $serviceName 2>&1 | Out-Null
        & $Binary -t uninstall -S $serviceName 2>&1 | Out-Null
    }
    if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
        & sc.exe delete $serviceName 2>&1 | Out-Null
    }
    Pop-Location
    if (-not $KeepTemp) {
        Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        Write-Host "The smoke test directory remains at $root"
    }
}
