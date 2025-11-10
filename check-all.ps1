#!/usr/bin/env pwsh
# Check everything: build lib + known examples, run doc tests, build docs

Write-Host "==> Building library..." -ForegroundColor Cyan
cargo build --lib
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n==> Building examples (pico2, no wifi)..." -ForegroundColor Cyan
$examples_no_wifi = @("blinky", "ir", "led_strip", "led_strip_snake", "led24x4_clock")
foreach ($example in $examples_no_wifi) {
    Write-Host "  - $example" -ForegroundColor DarkGray
    cargo build --example $example --no-default-features --features pico2
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "`n==> Building examples (pico2, with wifi)..." -ForegroundColor Cyan
$examples_wifi = @("clock_led4_wifi", "lcd_clock", "log_time")
foreach ($example in $examples_wifi) {
    Write-Host "  - $example" -ForegroundColor DarkGray
    cargo build --example $example --no-default-features --features pico2,wifi
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "`n==> Running doc tests..." -ForegroundColor Cyan
cargo test --doc
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n==> Building documentation..." -ForegroundColor Cyan
cargo doc --no-deps
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n==> All checks passed!" -ForegroundColor Green
