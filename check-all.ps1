#!/usr/bin/env pwsh
# Check everything: build lib + known examples, run doc tests, build docs

Write-Host "==> Building library..." -ForegroundColor Cyan
cargo build --lib --features pico2,arm --no-default-features
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n==> Building examples (pico2, arm, no wifi)..." -ForegroundColor Cyan
$examples_no_wifi = @("blinky", "ir", "led_strip", "led_strip_snake", "led24x4_clock")
foreach ($example in $examples_no_wifi) {
    Write-Host "  - $example" -ForegroundColor DarkGray
    cargo build --example $example --features pico2,arm --no-default-features
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "`n==> Building examples (pico2, arm, with wifi)..." -ForegroundColor Cyan
$examples_wifi = @("clock_led4_wifi", "lcd_clock", "log_time")
foreach ($example in $examples_wifi) {
    Write-Host "  - $example" -ForegroundColor DarkGray
    cargo build --example $example --features pico2,arm,wifi --no-default-features
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "`n==> Running doc tests..." -ForegroundColor Cyan
# Note: Using pico2,arm for tests instead of pico1,arm because embassy-rp from git main
# currently has a regression with Cortex-M0+ (tries to access MPU.rasr field that doesn't exist).
cargo test --doc --features pico2,arm,wifi --no-default-features
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "\n==> Building documentation..." -ForegroundColor Cyan
# Note: Using pico2,arm,wifi for docs instead of pico1,arm,wifi because embassy-rp from git main
# currently has a regression with Cortex-M0+ (tries to access MPU.rasr field that doesn't exist).
# The API is identical between pico1 and pico2, so these docs work for both platforms.
cargo doc --no-deps --features pico2,arm,wifi --no-default-features
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n==> Running QEMU embedded test..." -ForegroundColor Cyan
Push-Location tests\embedded
try {
    # Ensure QEMU is in PATH
    if (-not (Get-Command qemu-system-arm -ErrorAction SilentlyContinue)) {
        $env:PATH += ";C:\Program Files\qemu"
    }
    
    # Run the test and capture output
    $output = cargo run 2>&1 | Out-String
    
    if ($output -match "All tests passed!") {
        Write-Host "  PASS: Embedded tests passed in QEMU" -ForegroundColor Green
    } else {
        Write-Host "  FAIL: Embedded tests failed" -ForegroundColor Red
        Write-Host $output
        exit 1
    }
    
    if ($LASTEXITCODE -ne 0) { 
        Write-Host "  FAIL: QEMU test exited with error" -ForegroundColor Red
        exit $LASTEXITCODE 
    }
} finally {
    Pop-Location
}

Write-Host "`n==> All checks passed!" -ForegroundColor Green
