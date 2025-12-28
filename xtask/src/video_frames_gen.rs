//! Generate Rust code for embedded video frames from PNG files or video files.

use std::fs::{self, File};
use std::path::Path;
use std::process::Command;

pub fn generate_frames() -> Result<(), Box<dyn std::error::Error>> {
    generate_frames_from_pngs("santa")
}

pub fn generate_cat_frames() -> Result<(), Box<dyn std::error::Error>> {
    // Path to the cat video - check environment variable first, then default locations
    let video_path = std::env::var("CAT_VIDEO_PATH").unwrap_or_else(|_| {
        // Try WSL path first (most likely on your system)
        let wsl_path =
            "/mnt/c/Users/carlk/OneDrive/SkyDrive camera roll/PXL_20251227_031845967.mp4";
        if Path::new(wsl_path).exists() {
            eprintln!("Found video at: {}", wsl_path);
            return wsl_path.to_string();
        }

        // Try common OneDrive locations on Linux
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let candidates = [
            format!("{}/OneDrive/SkyDrive camera roll/cat.mp4", home),
            format!("{}/OneDrive/Camera Roll/cat.mp4", home),
            format!(
                "{}/.local/share/onedrive/SkyDrive camera roll/cat.mp4",
                home
            ),
        ];

        for candidate in &candidates {
            if Path::new(candidate).exists() {
                eprintln!("Found video at: {}", candidate);
                return candidate.clone();
            }
        }

        // Default fallback - show helpful error
        eprintln!("Could not find cat video in standard OneDrive locations.");
        eprintln!("Set CAT_VIDEO_PATH environment variable to specify the video location.");
        eprintln!("Example: CAT_VIDEO_PATH=\"/path/to/cat.mp4\" cargo xtask cat-frames-gen");
        wsl_path.to_string()
    });

    generate_video_frames(&video_path, "cat")
}

pub fn generate_hand_frames() -> Result<(), Box<dyn std::error::Error>> {
    let video_path = std::env::var("HAND_VIDEO_PATH").unwrap_or_else(|_| {
        let wsl_path =
            "/mnt/c/Users/carlk/OneDrive/SkyDrive camera roll/PXL_20251227_040453557.mp4";
        if Path::new(wsl_path).exists() {
            eprintln!("Found video at: {}", wsl_path);
            return wsl_path.to_string();
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let candidates = [
            format!("{}/OneDrive/SkyDrive camera roll/hand.mp4", home),
            format!("{}/OneDrive/Camera Roll/hand.mp4", home),
        ];

        for candidate in &candidates {
            if Path::new(candidate).exists() {
                eprintln!("Found video at: {}", candidate);
                return candidate.clone();
            }
        }

        eprintln!("Could not find hand video in standard OneDrive locations.");
        eprintln!("Set HAND_VIDEO_PATH environment variable to specify the video location.");
        wsl_path.to_string()
    });

    generate_video_frames(&video_path, "hand")
}

pub fn generate_clock_frames() -> Result<(), Box<dyn std::error::Error>> {
    let video_path = std::env::var("CLOCK_VIDEO_PATH").unwrap_or_else(|_| {
        let wsl_path = "/mnt/c/Users/carlk/Downloads/PXL_20251228_220608546.mp4";
        if Path::new(wsl_path).exists() {
            eprintln!("Found video at: {}", wsl_path);
            return wsl_path.to_string();
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let candidates = [
            format!("{}/OneDrive/SkyDrive camera roll/clock.mp4", home),
            format!("{}/Downloads/clock.mp4", home),
        ];

        for candidate in &candidates {
            if Path::new(candidate).exists() {
                eprintln!("Found video at: {}", candidate);
                return candidate.clone();
            }
        }

        eprintln!("Could not find clock video in standard locations.");
        eprintln!("Set CLOCK_VIDEO_PATH environment variable to specify the video location.");
        wsl_path.to_string()
    });

    generate_video_frames(&video_path, "clock")
}

fn generate_video_frames(video_path: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary directory for extracted frames
    let temp_dir = std::env::temp_dir().join(format!("{}_frames_12x8", name));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;

    eprintln!("Extracting frames from video: {}", video_path);
    eprintln!("Output directory: {}", temp_dir.display());

    // Use ffmpeg to extract frames at 10 FPS, scaled to 12x8
    let status = Command::new("ffmpeg")
        .args([
            "-i",
            &video_path,
            "-vf",
            "fps=10,scale=12:8:flags=lanczos",
            "-q:v",
            "2",
            &format!("{}/frame_%06d.png", temp_dir.display()),
        ])
        .status()?;

    if !status.success() {
        return Err("ffmpeg failed to extract frames".into());
    }

    // Count extracted frames
    let frame_count = fs::read_dir(&temp_dir)?.count();
    eprintln!("Extracted {} frames", frame_count);

    generate_frames_from_directory(&temp_dir, name, frame_count)
}

fn generate_frames_from_pngs(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let frames_dir =
        Path::new(&std::env::var("HOME")?).join("programs/ffmpeg-test/frames12x8_landscape");
    generate_frames_from_directory(&frames_dir, name, 65)
}

fn generate_frames_from_directory(
    frames_dir: &Path,
    name: &str,
    frame_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    const FRAME_DURATION_MILLIS: u64 = 100;
    let upper_name = name.to_uppercase();
    let frame_duration_name = format!("{}_FRAME_DURATION", upper_name);
    let frame_count_name = format!("{}_FRAME_COUNT", upper_name);
    let frames_name = format!("{}_FRAMES", upper_name);

    println!("// Video frames generated from PNG files ({} video)", name);
    println!("// Auto-generated - do not edit manually");
    println!();

    println!("#[allow(dead_code)]");
    println!(
        "// Frame duration for 10 FPS (100ms per frame)\nconst {}: Duration = Duration::from_millis({});",
        frame_duration_name, FRAME_DURATION_MILLIS
    );
    println!();

    println!("#[allow(dead_code)]");
    println!("const {}: usize = {};", frame_count_name, frame_count);
    println!();
    println!(
        "#[allow(dead_code)]\nconst {}: [([[RGB8; 12]; 8], Duration); {}] = [",
        frames_name, frame_count_name
    );

    for frame_num in 1..=frame_count {
        let filename = frames_dir.join(format!("frame_{:06}.png", frame_num));

        if !filename.exists() {
            eprintln!("Warning: {} not found, skipping", filename.display());
            continue;
        }

        let decoder = png::Decoder::new(File::open(&filename)?);
        let mut reader = decoder.read_info()?;
        let info = reader.info();

        if info.width != 12 || info.height != 8 {
            eprintln!(
                "Warning: {} has wrong dimensions ({}x{}), expected 12x8",
                filename.display(),
                info.width,
                info.height
            );
            continue;
        }

        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf)?;

        println!("    // Frame {}", frame_num);
        println!("    (");
        println!("        [");

        // Flip rows vertically (row 0 in PNG becomes row 7 in output)
        for row in (0..8).rev() {
            print!("        [");
            for col in 0..12 {
                let pixel_index = (row * 12 + col)
                    * match reader.info().color_type {
                        png::ColorType::Rgb => 3,
                        png::ColorType::Rgba => 4,
                        _ => panic!("Unsupported color type: {:?}", reader.info().color_type),
                    };

                let r = buf[pixel_index];
                let g = buf[pixel_index + 1];
                let b = buf[pixel_index + 2];

                print!("RGB8::new({}, {}, {})", r, g, b);
                if col < 11 {
                    print!(", ");
                }
            }
            println!("],");
        }

        println!("        ],");
        println!("        {},", frame_duration_name);
        print!("    )");
        if frame_num < frame_count {
            println!(",");
        } else {
            println!();
        }
    }

    println!("];");

    Ok(())
}
