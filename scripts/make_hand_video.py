#!/usr/bin/env python3
"""
Extract hand frames from hand_frames_data.rs and create a video with ffmpeg.
"""

import re
import subprocess
import tempfile
from pathlib import Path
from PIL import Image

def parse_rgb(text):
    """Parse RGB8::new(r, g, b) into (r, g, b) tuple."""
    match = re.search(r'RGB8::new\((\d+),\s*(\d+),\s*(\d+)\)', text)
    if match:
        return (int(match.group(1)), int(match.group(2)), int(match.group(3)))
    return None

def extract_frames(rust_file):
    """Extract all frames from the Rust data file."""
    with open(rust_file, 'r') as f:
        content = f.read()
    
    # Find all frames (arrays of 8 rows, each with 12 RGB values)
    frames = []
    current_frame = []
    current_row = []
    
    in_frame = False
    for line in content.split('\n'):
        # Start of a frame
        if '// Frame' in line:
            if current_frame:
                frames.append(current_frame)
            current_frame = []
            in_frame = True
            continue
        
        if in_frame:
            # Find all RGB values in this line
            rgb_values = re.findall(r'RGB8::new\(\d+,\s*\d+,\s*\d+\)', line)
            for rgb_str in rgb_values:
                rgb = parse_rgb(rgb_str)
                if rgb:
                    current_row.append(rgb)
            
            # End of row (contains '],' or '],')
            if '],' in line:
                if current_row:
                    current_frame.append(current_row)
                    current_row = []
    
    # Add the last frame
    if current_frame:
        frames.append(current_frame)
    
    return frames

def create_video(frames, output_path, scale=40, fps=10):
    """Create a video from frames using ffmpeg."""
    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_path = Path(tmpdir)
        
        # Create PNG files for each frame
        for idx, frame_data in enumerate(frames):
            # Create a 12x8 image
            img = Image.new('RGB', (12, 8))
            pixels = img.load()
            
            for row_idx, row in enumerate(frame_data):
                for col_idx, (r, g, b) in enumerate(row):
                    pixels[col_idx, row_idx] = (r, g, b)
            
            # Scale up for visibility
            img = img.resize((12 * scale, 8 * scale), Image.NEAREST)
            
            # Save with zero-padded filename
            img.save(tmp_path / f"frame_{idx:04d}.png")
        
        # Use ffmpeg to create video
        cmd = [
            'ffmpeg', '-y',
            '-framerate', str(fps),
            '-i', str(tmp_path / 'frame_%04d.png'),
            '-c:v', 'libx264',
            '-pix_fmt', 'yuv420p',
            '-crf', '18',
            str(output_path)
        ]
        
        print(f"Creating video with {len(frames)} frames at {fps} fps...")
        subprocess.run(cmd, check=True)
        print(f"Video saved to: {output_path}")

def main():
    script_dir = Path(__file__).parent
    repo_dir = script_dir.parent
    
    rust_file = repo_dir / 'hand_frames_data.rs'
    output_file = repo_dir / 'hand_video.mp4'
    
    if not rust_file.exists():
        print(f"Error: {rust_file} not found")
        return 1
    
    print(f"Extracting frames from {rust_file}...")
    frames = extract_frames(rust_file)
    print(f"Found {len(frames)} frames")
    
    if not frames:
        print("No frames found!")
        return 1
    
    # Validate frame dimensions
    for idx, frame in enumerate(frames):
        if len(frame) != 8:
            print(f"Warning: Frame {idx} has {len(frame)} rows (expected 8)")
        for row_idx, row in enumerate(frame):
            if len(row) != 12:
                print(f"Warning: Frame {idx}, row {row_idx} has {len(row)} pixels (expected 12)")
    
    create_video(frames, output_file, scale=40, fps=10)
    return 0

if __name__ == '__main__':
    exit(main())
