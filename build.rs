use std::{env, fs, path::PathBuf};

fn main() {
    // 1) Handle memory.x based on target
    let target = env::var("TARGET").unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    if target.starts_with("thumbv8m") {
        // Pico 2: copy our custom memory-pico2.x to OUT_DIR as memory.x
        let memory_x = fs::read_to_string("memory-pico2.x").expect("Failed to read memory-pico2.x");
        let dest = out_dir.join("memory.x");
        fs::write(&dest, memory_x).expect("Failed to write memory.x");
        println!("cargo:rustc-link-search={}", out_dir.display());
        println!("cargo:rerun-if-changed=memory-pico2.x");
    } else if target.starts_with("thumbv6m") {
        // Pico 1W: copy our custom memory-pico1w.x to OUT_DIR as memory.x
        let memory_x =
            fs::read_to_string("memory-pico1w.x").expect("Failed to read memory-pico1w.x");
        let dest = out_dir.join("memory.x");
        fs::write(&dest, memory_x).expect("Failed to write memory.x");
        println!("cargo:rustc-link-search={}", out_dir.display());
        println!("cargo:rerun-if-changed=memory-pico1w.x");
    }

    // 2) Try project-local .env (ignored by git)
    let _ = dotenvy::from_filename(".env");

    // 3) Fall back to HOME/.pico.env or HOME/.env (Windows: USERPROFILE)
    if env::var("WIFI_SSID").is_err()
        || env::var("WIFI_PASS").is_err()
        || env::var("UTC_OFFSET_MINUTES").is_err()
    {
        let home = env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .expect("Could not determine home directory (USERPROFILE/HOME not set)");
        let mut p = PathBuf::from(&home);
        p.push(".pico.env");
        if dotenvy::from_path(&p).is_err() {
            let mut p = PathBuf::from(&home);
            p.push(".env");
            let _ = dotenvy::from_path(&p);
        }
    }

    // 4) Require all vars (fail fast with clear message)
    let ssid =
        env::var("WIFI_SSID").expect("Missing WIFI_SSID (set in ./.env, ~/.pico.env, or ~/.env)");
    let pass =
        env::var("WIFI_PASS").expect("Missing WIFI_PASS (set in ./.env, ~/.pico.env, or ~/.env)");
    let utc_offset = env::var("UTC_OFFSET_MINUTES").expect(
        "Missing UTC_OFFSET_MINUTES (set in ./.env, ~/.pico.env, or ~/.env, e.g., -420 for PST)",
    );

    // 5) Expose as compile-time constants
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
