# Useful Commands

## Run

```bash
cargo run
```

## Generate Documentation

```bash
cargo doc --no-deps --open
```

## Emulation

```cmd
python3 -m pip install -r C:\deldir\1124\Renode_RP2040\visualization\requirements.txt
cd tests
renode --console run_clock.resc
startVisualization 1234
visualizationSetBoardElement led
visualizationLoadLayout @clock_layout.json 
s
```

```cmd
http://localhost:1234/
```
