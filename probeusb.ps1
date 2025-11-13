<#
.SYNOPSIS
    Helper for moving a CMSIS-DAP style debug probe between Windows and WSL.
.DESCRIPTION
    - No argument: shows who currently owns the probe.
    - 'wsl': attaches the probe to WSL (Windows loses access).
    - 'win': detaches from WSL so Windows regains the device.
    The script assumes exactly one debug probe is connected. If multiple USB
    devices are present, it tries to match names containing "probe", "CMSIS",
    or "DAP". Override the match via $env:PROBEUSB_PATTERN when needed.
#>
param(
    [Parameter(Position = 0)]
    [ValidateSet('win', 'wsl')]
    [string]$Target
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Invoke-Usbipd {
    param(
        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    $output = & usbipd @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "usbipd $($Arguments -join ' ') failed:`n$output"
    }
    return $output
}

function Get-ProbeDevice {
    $list = Invoke-Usbipd -Arguments @('wsl', 'list')
    $deviceLines = $list | Where-Object { $_ -match '^\s*\d+-\d+' }
    $devices = foreach ($line in $deviceLines) {
        $parts = $line.Trim() -split '\s{2,}'
        if ($parts.Length -lt 4) { continue }
        [PSCustomObject]@{
            BusId  = $parts[0]
            VidPid = $parts[1]
            State  = $parts[2]
            Name   = ($parts[3..($parts.Length - 1)] -join '  ')
        }
    }

    if (-not $devices) {
        throw 'No USB devices detected via usbipd.'
    }

    $pattern = $env:PROBEUSB_PATTERN
    if (-not $pattern) {
        $pattern = '(?i)(probe|cmsis|dap)'
    }

    $candidates = $devices | Where-Object { $_.Name -match $pattern }
    if (-not $candidates) {
        if ($devices.Count -eq 1) {
            return $devices[0]
        }
        throw "Unable to identify the probe. Set PROBEUSB_PATTERN to part of its name.`nDevices:`n$($devices | Format-Table -AutoSize | Out-String)"
    }

    if ($candidates.Count -gt 1) {
        throw "Multiple devices match '$pattern'. Tighten PROBEUSB_PATTERN.`nMatches:`n$($candidates | Format-Table -AutoSize | Out-String)"
    }

    return $candidates[0]
}

function Describe-State {
    param([string]$State)
    if ($State -eq 'Attached') {
        return 'WSL'
    }
    return 'Windows'
}

try {
    $device = Get-ProbeDevice
} catch {
    Write-Error $_
    exit 1
}

switch ($Target) {
    'wsl' {
        if ($device.State -eq 'Attached') {
            Write-Host "Probe already attached to WSL (bus $($device.BusId))."
            break
        }

        Write-Host "Attaching $($device.Name) on bus $($device.BusId) to WSL..."
        Invoke-Usbipd -Arguments @('wsl', 'attach', '--busid', $device.BusId)
        Write-Host 'Done. Windows now relinquishes the probe.'
    }
    'win' {
        if ($device.State -ne 'Attached') {
            Write-Host "Probe already owned by Windows (bus $($device.BusId))."
            break
        }

        Write-Host "Detaching $($device.Name) from WSL (bus $($device.BusId))..."
        Invoke-Usbipd -Arguments @('wsl', 'detach', '--busid', $device.BusId)
        Write-Host 'Done. Windows can now access the probe/COM port.'
    }
    Default {
        $owner = Describe-State -State $device.State
        Write-Host "Probe: $($device.Name)"
        Write-Host "BusID: $($device.BusId)"
        Write-Host "VID:PID: $($device.VidPid)"
        Write-Host "Owner: $owner"
        if ($owner -eq 'WSL') {
            Write-Host 'Run: powershell -File .\probeusb.ps1 win   # return to Windows'
        } else {
            Write-Host 'Run: powershell -File .\probeusb.ps1 wsl   # hand over to WSL'
        }
    }
}
