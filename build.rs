use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    // 1) Generate video frames data if building the video example
    // Check if we're building the video example by looking at CARGO_BIN_NAME or features
    let cargo_target_tmpdir = env::var("CARGO_TARGET_TMPDIR").ok();
    let should_generate = cargo_target_tmpdir
        .as_deref()
        .map(|s| s.contains("examples/video-"))
        .unwrap_or(false);

    if should_generate {
        eprintln!("Generating video frames data...");
        let output = Command::new("cargo")
            .args(["xtask", "video-frames-gen"])
            .output()
            .expect("Failed to run cargo xtask video-frames-gen");

        if output.status.success() {
            let frames_data = String::from_utf8_lossy(&output.stdout);
            fs::write("video_frames_data.rs", frames_data.as_bytes())
                .expect("Failed to write video_frames_data.rs");
            eprintln!("Video frames data generated successfully");
        } else {
            eprintln!(
                "Warning: Failed to generate video frames: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // Set up rerun trigger for video source directory
    if let Ok(home) = env::var("HOME") {
        let frames_dir = PathBuf::from(home).join("programs/ffmpeg-test/frames12x8_landscape");
        if frames_dir.exists() {
            println!("cargo:rerun-if-changed={}", frames_dir.display());
        }
    }

    // 2) Handle memory.x based on target
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
}
