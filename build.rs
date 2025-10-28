use std::{env, path::PathBuf};

fn main() {
    // 1) Try project-local .env (ignored by git)
    let _ = dotenvy::from_filename(".env");

    // 2) Fall back to HOME/.pico.env or HOME/.env (Windows: USERPROFILE)
    if env::var("WIFI_SSID").is_err() || env::var("WIFI_PASS").is_err() || env::var("UTC_OFFSET_MINUTES").is_err() {
        let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"))
            .expect("Could not determine home directory (USERPROFILE/HOME not set)");
        let mut p = PathBuf::from(&home);
        p.push(".pico.env");
        if dotenvy::from_path(&p).is_err() {
            let mut p = PathBuf::from(&home);
            p.push(".env");
            let _ = dotenvy::from_path(&p);
        }
    }

    // 3) Require all vars (fail fast with clear message)
    let ssid = env::var("WIFI_SSID")
        .expect("Missing WIFI_SSID (set in ./.env, ~/.pico.env, or ~/.env)");
    let pass = env::var("WIFI_PASS")
        .expect("Missing WIFI_PASS (set in ./.env, ~/.pico.env, or ~/.env)");
    let utc_offset = env::var("UTC_OFFSET_MINUTES")
        .expect("Missing UTC_OFFSET_MINUTES (set in ./.env, ~/.pico.env, or ~/.env, e.g., -420 for PST)");


    // 4) Expose as compile-time constants
    println!("cargo:rustc-env=WIFI_SSID={ssid}");
    println!("cargo:rustc-env=WIFI_PASS={pass}");
    println!("cargo:rustc-env=UTC_OFFSET_MINUTES={utc_offset}");

    // Optional: don't rebuild unless these change
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASS");
    println!("cargo:rerun-if-env-changed=UTC_OFFSET_MINUTES");
    println!("cargo:rerun-if-env-changed=DST_OFFSET_MINUTES");
    println!("cargo:rerun-if-changed=.env");
}