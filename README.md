# Clock

## State Diagram

```mermaid
stateDiagram-v2

   %% Style overrides

    style DisplayHoursMinutes fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    style DisplayMinutesSeconds fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    style ShowSeconds fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    style ShowMinutes fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold    
    style ShowHours fill:#000,stroke:#333,stroke-width:2px,color:#ff4444,font-family:"Courier New",font-size:18px,font-weight:bold
    

    DisplayHoursMinutes --> DisplayMinutesSeconds : Tap
    DisplayMinutesSeconds --> DisplayHoursMinutes : Tap
    DisplayHoursMinutes --> ShowSeconds : Press & Release
    DisplayMinutesSeconds --> ShowSeconds : Press & Release
    ShowSeconds --> ShowMinutes : Tap
    ShowSeconds --> EditSeconds : Press
    EditSeconds --> ShowSeconds : Release
    ShowMinutes --> ShowHours : Tap
    ShowMinutes --> EditMinutes : Press
    EditMinutes --> ShowMinutes : Release
    ShowHours --> DisplayHoursMinutes : Tap
    ShowHours --> EditHours : Press
    EditHours --> ShowHours : Release

    DisplayHoursMinutes: HHMM
    DisplayMinutesSeconds: MMSS
    state "&nbsp;&nbsp;✨SS✨&nbsp;&nbsp;" as ShowSeconds
    EditSeconds: *to 00*
    state "&nbsp;&nbsp;&nbsp;&nbsp;✨MM✨" as ShowMinutes    
    EditMinutes: *increments*
    state "✨HH✨&nbsp;&nbsp;&nbsp;&nbsp;" as ShowHours
    EditHours: *increments*

```

Note: ✨ indicates blinking.

### Display Modes

* HHMM
* MMSS

Tap: Switch between the two display modes.
Press & Release: Move to the edit modes.

### Edit Modes

* SS (blinking)
* MM (blinking)
* HH (blinking)

Tap: Move through the three edit modes and then return to the display modes.
Press: Change the value. Release when the value is right. Seconds go to 00. Minutes and hours increment quickly.

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
