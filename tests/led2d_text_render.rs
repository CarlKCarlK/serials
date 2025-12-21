#![cfg(feature = "host")]

use png::{BitDepth, ColorType, Decoder, Encoder};
use device_kit::led2d::{Frame, Led2dFont, render_text_to_frame};
use smart_leds::{RGB8, colors};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const REFERENCE_DIR: &str = "tests/data/text_render";

#[test]
fn font3x4_on_12x4_matches_reference() {
    run_render_test::<4, 12>(
        "font3x4_12x4",
        Led2dFont::Font3x4Trim,
        "RUST",
        &four_colors(),
    );
}

#[test]
fn font4x6_on_12x4_clips_bottom_matches_reference() {
    run_render_test::<4, 12>(
        "font4x6_12x4",
        Led2dFont::Font4x6,
        "RUST\ntwo",
        &four_colors(),
    );
}

#[test]
fn font6x10_on_24x16_clips_and_colors_cycle() {
    run_render_test::<16, 24>(
        "font6x10_24x16",
        Led2dFont::Font6x10,
        "Hello Rust\nWrap me",
        &[colors::CYAN, colors::MAGENTA],
    );
}

#[test]
fn font5x8_on_600x800_fibonacci() {
    run_render_test_heap::<600, 800>(
        "font5x8_600x800_fibonacci",
        Led2dFont::Font5x8,
        "1\n1\n2\n3\n5\n8\n13\n21\n34\n55\n89\n144\n233\n377\n610\n987\n1597\n2584\n4181\n6765",
        &[colors::GREEN, colors::YELLOW, colors::ORANGE],
    );
}

#[test]
fn font3x4_on_12x4_no_colors_defaults_to_white() {
    run_render_test::<4, 12>(
        "font3x4_12x4_white",
        Led2dFont::Font3x4Trim,
        "RUST",
        &[],
    );
}

fn run_render_test<const ROWS: usize, const COLS: usize>(
    name: &str,
    font: Led2dFont,
    text: &str,
    colors: &[RGB8],
) {
    let mut frame: Frame<ROWS, COLS> = Frame::new();
    render_text_to_frame(&mut frame, &font.to_font(), text, colors, (0, 0))
        .expect("render must succeed");

    if let Some(dir) = generation_dir() {
        let output_path = dir.join(format!("{name}.png"));
        write_png(&frame, &output_path);
        println!("wrote {name} to {}", output_path.display());
        return;
    }

    let reference_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(REFERENCE_DIR)
        .join(format!("{name}.png"));
    let reference = read_png::<ROWS, COLS>(&reference_path);
    assert_eq!(
        frame_pixels(&frame),
        reference,
        "rendered output for {name} did not match reference at {}",
        reference_path.display()
    );
}

#[expect(unsafe_code, reason = "heap allocation for large test frames")]
fn run_render_test_heap<const ROWS: usize, const COLS: usize>(
    name: &str,
    font: Led2dFont,
    text: &str,
    colors: &[RGB8],
) {
    // Allocate on heap to handle large frames that would overflow the stack
    let frame_vec: Vec<RGB8> = vec![smart_leds::RGB8::default(); ROWS * COLS];
    let mut frame_box = frame_vec.into_boxed_slice();
    let frame_ptr = frame_box.as_mut_ptr() as *mut [[RGB8; COLS]; ROWS];
    let frame_ref: &mut Frame<ROWS, COLS> = unsafe { &mut *(frame_ptr as *mut Frame<ROWS, COLS>) };

    render_text_to_frame(frame_ref, &font.to_font(), text, colors, (0, 0))
        .expect("render must succeed");

    if let Some(dir) = generation_dir() {
        let output_path = dir.join(format!("{name}.png"));
        write_png(frame_ref, &output_path);
        println!("wrote {name} to {}", output_path.display());
        return;
    }

    let reference_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(REFERENCE_DIR)
        .join(format!("{name}.png"));
    let reference = read_png::<ROWS, COLS>(&reference_path);
    assert_eq!(
        frame_pixels(frame_ref),
        reference,
        "rendered output for {name} did not match reference at {}",
        reference_path.display()
    );
}

fn generation_dir() -> Option<PathBuf> {
    let env_value = std::env::var("DEVICE_KIT_GENERATE_TEXT_PNGS").ok()?;
    let dir = if env_value.is_empty() {
        let mut path = std::env::temp_dir();
        path.push("device-kit-text-pngs");
        path
    } else {
        PathBuf::from(env_value)
    };
    std::fs::create_dir_all(&dir).expect("failed to create PNG output directory");
    Some(dir)
}

fn write_png<const ROWS: usize, const COLS: usize>(frame: &Frame<ROWS, COLS>, path: &Path) {
    let file = File::create(path).expect("failed to create PNG file");
    let mut encoder = Encoder::new(BufWriter::new(file), COLS as u32, ROWS as u32);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().expect("failed to write PNG header");
    writer
        .write_image_data(&frame_pixels(frame))
        .expect("failed to write PNG data");
}

fn read_png<const ROWS: usize, const COLS: usize>(path: &Path) -> Vec<u8> {
    let file =
        File::open(path).unwrap_or_else(|_| panic!("missing reference PNG at {}", path.display()));
    let decoder = Decoder::new(file);
    let mut reader = decoder.read_info().expect("failed to read PNG");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .expect("failed to decode PNG");
    assert_eq!(info.width, COLS as u32, "reference PNG width mismatch");
    assert_eq!(info.height, ROWS as u32, "reference PNG height mismatch");
    buffer[..info.buffer_size()].to_vec()
}

fn frame_pixels<const ROWS: usize, const COLS: usize>(frame: &Frame<ROWS, COLS>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(ROWS * COLS * 3);
    for row in 0..ROWS {
        for col in 0..COLS {
            let pixel = frame.0[row][col];
            bytes.push(pixel.r);
            bytes.push(pixel.g);
            bytes.push(pixel.b);
        }
    }
    bytes
}

fn four_colors() -> [RGB8; 4] {
    [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW]
}
