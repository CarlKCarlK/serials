use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let tmp_dir = manifest_dir.join("../target/tmp");
    fs::create_dir_all(&tmp_dir).expect("Failed to create target/tmp for temporary files");
}
