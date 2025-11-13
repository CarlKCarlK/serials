param(
    [ValidateSet('win', 'wsl')]
    [string]$Mode
)

# Look for the Raspberry Pi Debug Probe (CMSIS-DAP) by VID:PID 2e8a:000c
$probeLine = usbipd list | Select-String '2e8a:000c'

if (-not $probeLine) {
    Write-Host "No Raspberry Pi Debug Probe (VID:PID 2e8a:000c) found." -ForegroundColor Yellow
    exit 1
}

$line = $probeLine.ToString()

# Grab BUSID (pattern like 6-4) from the start of the line. Some usbipd versions add
# leading whitespace, so rely on a regex rather than positional splitting.
$busMatch = [regex]::Match($line, '^\s*(?<busid>\d+-\d+)')
if (-not $busMatch.Success) {
    Write-Host "Unable to parse BUSID from usbipd output: '$line'" -ForegroundColor Red
    exit 1
}
$busId = $busMatch.Groups['busid'].Value

# Extract "STATE" via regex: everything after double-space after the device name
# We match: "2e8a:000c  <device text>  <state>"
$match = [regex]::Match($line, '2e8a:000c\s+(?<device>.+?)\s{2,}(?<state>.+)$')
if ($match.Success) {
    $deviceName = $match.Groups['device'].Value.Trim()
    $state      = $match.Groups['state'].Value.Trim()
} else {
    $deviceName = "Unknown device name"
    $state      = "Unknown"
}

switch ($Mode) {
    'wsl' {
        if ($state -like 'Attached*') {
            Write-Host "Probe is already attached to WSL: $state" -ForegroundColor Green
            break
        }

        Write-Host "Attaching probe $busId to WSL..." -ForegroundColor Cyan
        usbipd attach --busid $busId --wsl

        if ($LASTEXITCODE -eq 0) {
            Write-Host "Probe $busId attached to WSL (default distro)." -ForegroundColor Green
        } else {
            Write-Host "Failed to attach probe $busId to WSL." -ForegroundColor Red
        }
    }

    'win' {
        if ($state -like 'Attached*') {
            Write-Host "Detaching probe $busId from WSL so Windows can use it..." -ForegroundColor Cyan
            usbipd detach --busid $busId

            if ($LASTEXITCODE -eq 0) {
                Write-Host "Probe $busId detached from WSL; Windows now owns it." -ForegroundColor Green
            } else {
                Write-Host "Failed to detach probe $busId from WSL." -ForegroundColor Red
            }
        } else {
            Write-Host "Probe is not attached to WSL (state: '$state'). Windows already owns it." -ForegroundColor Green
        }
    }

    default {
        Write-Host "Found Debug Probe:" -ForegroundColor Cyan
        Write-Host "  BUSID : $busId"
        Write-Host "  Device: $deviceName"
        Write-Host "  State : $state"

        if ($state -like 'Attached*') {
            Write-Host "→ Probe is currently attached to WSL." -ForegroundColor Yellow
        } elseif ($state -like 'Shared*') {
            Write-Host "→ Probe is shareable and currently usable by Windows; WSL can attach it." -ForegroundColor Yellow
        } elseif ($state -like 'Not shared*') {
            Write-Host "→ Probe is only visible to Windows (not shared with WSL yet)." -ForegroundColor Yellow
        } else {
            Write-Host "→ Probe state is unrecognized; run 'usbipd list' for details." -ForegroundColor Yellow
        }
    }
}
