//! This build script requests that `cargo` re-build the crate whenever `memory.x` is changed.
//! `memory.x`is a linker script--a text file telling the final step of the compilation process
//! how modules and program sections (parts of the program) should be located in memory when loaded
//! on hardware.
//! Linker scripts like `memory.x` are not normally a part of the build process and changes to it
//! would ordinarily be ignored by the build process.

use std::{env, fs::File, io::Write, path::PathBuf};

use chrono::{Local, Timelike};

fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Put `memory.x` in our output directory and ensure it's on the linker search path.
    let out =
        &PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR environment variable is not set"));
    File::create(out.join("memory.x"))?.write_all(include_bytes!("memory.x"))?;
    println!("cargo:rustc-link-search={}", out.display());

    // Tell `cargo` to rebuild project if `memory.x` linker script file changes
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rerun-if-changed=build.rs"); // Re-run if this file changes
    println!("cargo:rerun-if-changed=*"); // Re-run if any file in the project changes

    // Put the current millis since the Epoch into an environment variable
    let now = Local::now();
    // Calculate the time since local midnight
    #[expect(clippy::arithmetic_side_effects, reason = "Will never overflow")]
    let millis_since_midnight = u64::from(now.hour()) * 60 * 60 * 1000  // Hours to milliseconds
        + u64::from(now.minute()) * 60 * 1000                          // Minutes to milliseconds
        + u64::from(now.second()) * 1000                              // Seconds to milliseconds
        + u64::from(now.timestamp_subsec_millis()) // Milliseconds
        + 4000; // Add 4 seconds to the time to allow for the build process
    println!("cargo:rustc-env=BUILD_TIME={millis_since_midnight}");

    Ok(())
}
