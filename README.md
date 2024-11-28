# Clock

## State Diagram

```mermaid
stateDiagram-v2

   %% Style overrides

    style HoursMinutes fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    style MinutesSeconds fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    style ShowSeconds fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    style ShowMinutes fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold    
    style ShowHours fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    

    HoursMinutes --> MinutesSeconds : Tap
    MinutesSeconds --> HoursMinutes : Tap
    HoursMinutes --> ShowSeconds : Press & Release
    MinutesSeconds --> ShowSeconds : Press & Release
    ShowSeconds --> ShowMinutes : Tap
    ShowSeconds --> EditSeconds : Press
    EditSeconds --> ShowSeconds : Release
    ShowMinutes --> ShowHours : Tap
    ShowMinutes --> EditMinutes : Press
    EditMinutes --> ShowMinutes : Release
    ShowHours --> HoursMinutes : Tap
    ShowHours --> EditHours : Press
    EditHours --> ShowHours : Release

    HoursMinutes: HHMM
    MinutesSeconds: MMSS
    state "&nbsp;&nbsp;✨SS✨&nbsp;&nbsp;" as ShowSeconds
    EditSeconds: *to 00*
    state "&nbsp;&nbsp;&nbsp;&nbsp;✨MM✨" as ShowMinutes    
    EditMinutes: *increments*
    state "✨HH✨&nbsp;&nbsp;&nbsp;&nbsp;" as ShowHours
    EditHours: *increments*

```

Note: ✨ indicates blinking.

### Display Modes

* `HHMM`
* `MMSS`

**Tap**: Switch between the two display modes.

**Press & Release**: Move to the edit modes.

### Edit Modes (blinking)

<!-- markdownlint-disable MD038 -->
* ✨` SS `✨
* ✨`  MM`✨
* ✨`HH  `✨

**Tap**: Move through the three edit modes and then return to the display modes.

**Press**: Change the value. Seconds go to `00`. Minutes and hours increment quickly.

**Release**: When the value is what you wish.

## Tools & Debugging

This is project is setup to use `probe-rs`. The setup is based on
<https://github.com/U007D/blinky_probe/tree/main> from the
Embedded Rust Hardware Debug Probe workshop taught at the
Seattle Rust User Group in November 2024.

## License

Licensed under either:

* MIT license (see LICENSE-MIT file)
* Apache License, Version 2.0 (see LICENSE-APACHE file)
  at your option.
