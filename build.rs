//! This build script requests that `cargo` re-build the crate whenever `memory.x` is changed.
//! `memory.x`is a linker script--a text file telling the final step of the compilation process
//! how modules and program sections (parts of the program) should be located in memory when loaded
//! on hardware.
//! Linker scripts like `memory.x` are not normally a part of the build process and changes to it
//! would ordinarily be ignored by the build process.

use std::{env, fs::File, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Put `memory.x` in our output directory and ensure it's on the linker search path.
    let out =
        &PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR environment variable is not set"));
    File::create(out.join("memory.x"))?.write_all(include_bytes!("memory.x"))?;
    println!("cargo:rustc-link-search={}", out.display());

    // Tell `cargo` to rebuild project if `memory.x` linker script file changes
    println!("cargo:rerun-if-changed=memory.x");

    Ok(())
}
