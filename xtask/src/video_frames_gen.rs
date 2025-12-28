//! Generate Rust code for embedded video frames from PNG files.

use std::fs::File;
use std::path::Path;

pub fn generate_frames() -> Result<(), Box<dyn std::error::Error>> {
    let frames_dir =
        Path::new(&std::env::var("HOME")?).join("programs/ffmpeg-test/frames12x8_landscape");

    println!("// Video frames generated from PNG files");
    println!("// Auto-generated - do not edit manually");
    println!();
    println!("const VIDEO_FRAMES: [VideoFrame; FRAME_COUNT] = [");

    for frame_num in 1..=65 {
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
        println!("    [");

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

        print!("    ]");
        if frame_num < 65 {
            println!(",");
        } else {
            println!();
        }
    }

    println!("];");

    Ok(())
}
