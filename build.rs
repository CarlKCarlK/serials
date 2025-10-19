use std::{env, path::PathBuf};

fn main() {
    // 1) Try project-local .env (ignored by git)
    let _ = dotenvy::from_filename(".env");

    // 2) Fall back to HOME/.pico.env (Windows: USERPROFILE)
    if env::var("WIFI_SSID").is_err() || env::var("WIFI_PASS").is_err() || env::var("TIMEZONE").is_err() {
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
    let timezone = env::var("TIMEZONE")
        .expect("Missing TIMEZONE (set in ./.env or ~/.pico.env, e.g., America/Los_Angeles)");

    // 4) Expose as compile-time constants
    println!("cargo:rustc-env=WIFI_SSID={ssid}");
    println!("cargo:rustc-env=WIFI_PASS={pass}");
    println!("cargo:rustc-env=TIMEZONE={timezone}");

    // Optional: don't rebuild unless these change
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASS");
    println!("cargo:rerun-if-env-changed=TIMEZONE");
    println!("cargo:rerun-if-changed=.env");
}