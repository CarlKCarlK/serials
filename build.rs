use std::{env, fs, path::PathBuf};

fn main() {
    // 1) Handle memory.x based on target
    let target = env::var("TARGET").unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    if target.starts_with("thumbv8m") {
        // Pico 2 ARM: copy our custom memory-pico2.x to OUT_DIR as memory.x
        let memory_x = fs::read_to_string("memory-pico2.x").expect("Failed to read memory-pico2.x");
        let dest = out_dir.join("memory.x");
        fs::write(&dest, memory_x).expect("Failed to write memory.x");
        println!("cargo:rustc-link-search={}", out_dir.display());
        println!("cargo:rerun-if-changed=memory-pico2.x");
    } else if target.starts_with("riscv32imac") {
        // Pico 2 RISC-V: copy our custom memory-pico2-riscv.x to OUT_DIR as memory.x
        let memory_x = fs::read_to_string("memory-pico2-riscv.x")
            .expect("Failed to read memory-pico2-riscv.x");
        let dest = out_dir.join("memory.x");
        fs::write(&dest, memory_x).expect("Failed to write memory.x");
        println!("cargo:rustc-link-search={}", out_dir.display());
        println!("cargo:rerun-if-changed=memory-pico2-riscv.x");
    } else if target.starts_with("thumbv6m") {
        // Pico 1W: copy our custom memory-pico1w.x to OUT_DIR as memory.x
        let memory_x =
            fs::read_to_string("memory-pico1w.x").expect("Failed to read memory-pico1w.x");
        let dest = out_dir.join("memory.x");
        fs::write(&dest, memory_x).expect("Failed to write memory.x");
        println!("cargo:rustc-link-search={}", out_dir.display());
        println!("cargo:rerun-if-changed=memory-pico1w.x");
    }

    // 2) Load optional env files (still supported for convenience)
    let _ = dotenvy::from_filename(".env");
    load_home_env(".pico.env");
    load_home_env(".env");

    // 3) Provide fallbacks so Wi-Fi/clock features can compile without .env
    let wifi_ssid = env_or_default("WIFI_SSID", "");
    let wifi_pass = env_or_default("WIFI_PASS", "");
    let utc_offset = env_or_default("UTC_OFFSET_MINUTES", "0");

    // Warn only if Wi-Fi was explicitly enabled but credentials are missing.
    if env::var_os("CARGO_FEATURE_WIFI").is_some() {
        if wifi_ssid.is_empty() {
            println!(
                "cargo:warning=WIFI feature enabled but WIFI_SSID is not set; using empty string"
            );
        }
        if wifi_pass.is_empty() {
            println!(
                "cargo:warning=WIFI feature enabled but WIFI_PASS is not set; using empty string"
            );
        }
    }

    // 4) Expose as compile-time constants
    println!("cargo:rustc-env=WIFI_SSID={wifi_ssid}");
    println!("cargo:rustc-env=WIFI_PASS={wifi_pass}");
    println!("cargo:rustc-env=UTC_OFFSET_MINUTES={utc_offset}");

    // Optional: don't rebuild unless these change
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASS");
    println!("cargo:rerun-if-env-changed=UTC_OFFSET_MINUTES");
    println!("cargo:rerun-if-env-changed=DST_OFFSET_MINUTES");
    println!("cargo:rerun-if-changed=.env");
}

fn load_home_env(file: &str) {
    let home = match env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) {
        Some(path) => PathBuf::from(path),
        None => return,
    };
    let path = home.join(file);
    let _ = dotenvy::from_path(&path);
}

fn env_or_default(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
