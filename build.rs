use std::{env, fs, path::PathBuf};

fn main() {
    // Handle WiFi credentials (only needed for WiFi-enabled features)
    if env::var("CARGO_FEATURE_WIFI").is_ok() {
        // 1) Try project-local .env (ignored by git)
        let _ = dotenvy::from_filename(".env");

        // 2) Fall back to HOME/.pico.env (Windows: USERPROFILE)
        if env::var("WIFI_SSID").is_err() || env::var("WIFI_PASS").is_err() || env::var("UTC_OFFSET_MINUTES").is_err() {
            let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"))
                .expect("Could not determine home directory (USERPROFILE/HOME not set)");
            let mut p = PathBuf::from(home);
            p.push(".pico.env");
            let _ = dotenvy::from_path(&p);
        }

        // 3) Require all vars (fail fast with clear message)
        let ssid = env::var("WIFI_SSID")
            .expect("Missing WIFI_SSID (set in ./.env or ~/.pico.env)");
        let pass = env::var("WIFI_PASS")
            .expect("Missing WIFI_PASS (set in ./.env or ~/.pico.env)");
        let utc_offset = env::var("UTC_OFFSET_MINUTES")
            .expect("Missing UTC_OFFSET_MINUTES (set in ./.env or ~/.pico.env, e.g., -420 for PST)");

        // 4) Expose as compile-time constants
        println!("cargo:rustc-env=WIFI_SSID={ssid}");
        println!("cargo:rustc-env=WIFI_PASS={pass}");
        println!("cargo:rustc-env=UTC_OFFSET_MINUTES={utc_offset}");
    }

    // Handle memory.x based on hardware target
    let memory_x_content = if env::var("CARGO_FEATURE_RP2350").is_ok() {
        // Pico 2 / Pico 2W - 520KB RAM
        r#"MEMORY
{
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 4096K - 0x100
    RAM   : ORIGIN = 0x20000000, LENGTH = 520K
}
"#
    } else {
        // Pico 1W - 264KB RAM
        r#"MEMORY
{
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100
    RAM   : ORIGIN = 0x20000000, LENGTH = 264K
}
"#
    };

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("memory.x"), memory_x_content)
        .expect("Failed to write memory.x");
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    // Optional: don't rebuild unless these change
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASS");
    println!("cargo:rerun-if-env-changed=UTC_OFFSET_MINUTES");
    println!("cargo:rerun-if-env-changed=DST_OFFSET_MINUTES");
    println!("cargo:rerun-if-changed=.env");
}