use std::{env, path::PathBuf};

fn main() {
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

    // Optional DST parameters
    let dst_offset = env::var("DST_OFFSET_MINUTES").unwrap_or_else(|_| "0".to_string());
    let dst_start = env::var("DST_START").unwrap_or_else(|_| "".to_string());
    let dst_end = env::var("DST_END").unwrap_or_else(|_| "".to_string());

    // 4) Expose as compile-time constants
    println!("cargo:rustc-env=WIFI_SSID={ssid}");
    println!("cargo:rustc-env=WIFI_PASS={pass}");
    println!("cargo:rustc-env=UTC_OFFSET_MINUTES={utc_offset}");
    println!("cargo:rustc-env=DST_OFFSET_MINUTES={dst_offset}");
    println!("cargo:rustc-env=DST_START={dst_start}");
    println!("cargo:rustc-env=DST_END={dst_end}");

    // Optional: don't rebuild unless these change
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASS");
    println!("cargo:rerun-if-env-changed=UTC_OFFSET_MINUTES");
    println!("cargo:rerun-if-env-changed=DST_OFFSET_MINUTES");
    println!("cargo:rerun-if-env-changed=DST_START");
    println!("cargo:rerun-if-env-changed=DST_END");
    println!("cargo:rerun-if-changed=.env");
}