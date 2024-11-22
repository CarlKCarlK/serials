# Clock

## State Diagram

```mermaid
stateDiagram-v2
    Live_HH_MM --> Display_MM_SS : Tap
    Display_MM_SS --> Live_HH_MM : Tap
    Live_HH_MM --> Display_SS : Hold_&_Release
    Display_MM_SS --> Display_SS : Hold_&_Release
    Display_SS --> Display_MM : Tap
    Display_SS --> SS_To_00 : Hold
    SS_To_00 --> Display_SS : Release
    Display_MM --> Display_HH : Tap
    Display_MM --> MM_Increment : Hold
    MM_Increment --> Display_MM : Release
    Display_HH --> Live_HH_MM : Tap
    Display_HH --> HH_Increment : Hold
    HH_Increment --> Display_HH : Release
    
```

Use a quick press to switch between:

* HH:MM
* MM:SS
* View/edit seconds
* View/edit minutes
* View/edit hours

Do a long hold to start editing.

With seconds, it will display "00". Release at the top of the minute.
With minutes, it will change the value. Release when the minutes is right.
With hours, it will change the value. Release when the hour is right.

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
