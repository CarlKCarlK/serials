//! Build automation tasks for the serials project.
//!
//! Run with: `cargo xtask <command>`

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation for serials project", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all checks: build lib, examples, run tests, generate docs
    CheckAll,
    /// Build library with specified features
    Build {
        #[arg(long, default_value = "pico1")]
        board: Board,
        #[arg(long, default_value = "arm")]
        arch: Arch,
        #[arg(long)]
        wifi: bool,
    },
    /// Build an example
    Example {
        /// Example name (e.g., blinky, clock_lcd)
        name: String,
        #[arg(long, default_value = "pico1")]
        board: Board,
        #[arg(long, default_value = "arm")]
        arch: Arch,
        #[arg(long)]
        wifi: bool,
    },
    /// Build UF2 firmware file for flashing to Pico
    Uf2 {
        /// Example name (e.g., blinky, clock_lcd)
        name: String,
        #[arg(long, default_value = "pico1")]
        board: Board,
        #[arg(long, default_value = "arm")]
        arch: Arch,
        #[arg(long)]
        wifi: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Board {
    Pico1,
    Pico2,
}

impl std::fmt::Display for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Board::Pico1 => write!(f, "pico1"),
            Board::Pico2 => write!(f, "pico2"),
        }
    }
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Arch {
    Arm,
    Riscv,
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arch::Arm => write!(f, "arm"),
            Arch::Riscv => write!(f, "riscv"),
        }
    }
}

impl Arch {
    fn target(&self, board: Board) -> &'static str {
        match (board, self) {
            (Board::Pico1, Arch::Arm) => "thumbv6m-none-eabi",
            (Board::Pico2, Arch::Arm) => "thumbv8m.main-none-eabihf",
            (Board::Pico2, Arch::Riscv) => "riscv32imac-unknown-none-elf",
            (Board::Pico1, Arch::Riscv) => panic!("Pico 1 does not support RISC-V"),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::CheckAll => check_all(),
        Commands::Build { board, arch, wifi } => build_lib(board, arch, wifi),
        Commands::Example {
            name,
            board,
            arch,
            wifi,
        } => build_example(&name, board, arch, wifi),
        Commands::Uf2 {
            name,
            board,
            arch,
            wifi,
        } => build_uf2(&name, board, arch, wifi),
    }
}

fn check_all() -> ExitCode {
    let workspace_root = workspace_root();
    let examples = discover_examples(&workspace_root);
    let no_wifi_examples: Vec<_> = examples
        .iter()
        .filter(|example| !example.wifi_required)
        .collect();
    let arch = Arch::Arm;
    let board_pico2 = Board::Pico2;
    let board_pico1 = Board::Pico1;
    let target_pico2 = arch.target(board_pico2);
    let target_pico1 = arch.target(board_pico1);
    let features_no_wifi = build_features(board_pico2, arch, false);
    let features_wifi_pico2 = build_features(board_pico2, arch, true);
    let features_wifi_pico1 = build_features(board_pico1, arch, true);

    println!("{}", "==> Running doc tests...".cyan());
    if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
        "test",
        "--doc",
        "--target",
        target_pico2,
        "--features",
        features_wifi_pico2.as_str(),
        "--no-default-features",
    ])) {
        return ExitCode::FAILURE;
    }

    println!("\n{}", "==> Running unit tests...".cyan());
    let host_target = host_target();
    match host_target.as_deref() {
        Some(target) => {
            println!(
                "  {}",
                format!("Using host target: {target}").bright_black()
            );
        }
        None => {
            println!(
                "{}",
                "  Unable to detect host target; relying on cargo default.".bright_black()
            );
        }
    }

    let mut unit_test_cmd = Command::new("cargo");
    unit_test_cmd
        .current_dir(&workspace_root)
        .args(["test", "--lib"]);

    if let Some(target) = host_target {
        unit_test_cmd.arg("--target").arg(target);
    }

    unit_test_cmd.args(["--no-default-features", "--features", "host"]);

    if !run_command(&mut unit_test_cmd) {
        return ExitCode::FAILURE;
    }

    println!("\n{}", "==> Building library...".cyan());
    if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
        "build",
        "--lib",
        "--target",
        target_pico2,
        "--features",
        features_no_wifi.as_str(),
        "--no-default-features",
    ])) {
        return ExitCode::FAILURE;
    }

    println!(
        "\n{}",
        "==> Building examples (pico2, arm, no wifi)...".cyan()
    );
    for example in &no_wifi_examples {
        println!("  {}", format!("- {}", example.name).bright_black());
        if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
            "build",
            "--example",
            &example.name,
            "--target",
            target_pico2,
            "--features",
            features_no_wifi.as_str(),
            "--no-default-features",
        ])) {
            return ExitCode::FAILURE;
        }
    }

    println!(
        "\n{}",
        "==> Building examples (pico2, arm, with wifi)...".cyan()
    );
    for example in &examples {
        println!("  {}", format!("- {}", example.name).bright_black());
        if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
            "build",
            "--example",
            &example.name,
            "--target",
            target_pico2,
            "--features",
            features_wifi_pico2.as_str(),
            "--no-default-features",
        ])) {
            return ExitCode::FAILURE;
        }
    }

    println!(
        "\n{}",
        "==> Building examples (pico1, arm, with wifi)...".cyan()
    );
    for example in &examples {
        println!("  {}", format!("- {}", example.name).bright_black());
        if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
            "build",
            "--example",
            &example.name,
            "--target",
            target_pico1,
            "--features",
            features_wifi_pico1.as_str(),
            "--no-default-features",
        ])) {
            return ExitCode::FAILURE;
        }
    }

    println!("\n{}", "==> Building compile-only tests...".cyan());
    let compile_tests = ["led12x4"];
    for test in &compile_tests {
        println!("  {}", format!("- {test}").bright_black());
        if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
            "check",
            "--bin",
            test,
            "--target",
            target_pico1,
            "--features",
            "pico1,arm,wifi",
            "--no-default-features",
        ])) {
            return ExitCode::FAILURE;
        }
    }

    println!("\n{}", "==> Building documentation...".cyan());
    if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
        "doc",
        "--target",
        target_pico2,
        "--no-deps",
        "--features",
        features_wifi_pico2.as_str(),
        "--no-default-features",
    ])) {
        return ExitCode::FAILURE;
    }

    println!("\n{}", "==> All checks passed! 🎉".green().bold());
    ExitCode::SUCCESS
}

fn build_lib(board: Board, arch: Arch, wifi: bool) -> ExitCode {
    let workspace_root = workspace_root();
    let target = arch.target(board);
    let features = build_features(board, arch, wifi);
    println!(
        "{}",
        format!("Building library with features: {features}").cyan()
    );

    if run_command(Command::new("cargo").current_dir(&workspace_root).args([
        "build",
        "--lib",
        "--target",
        target,
        "--features",
        &features,
        "--no-default-features",
    ])) {
        println!("{}", "Build successful! ✨".green());
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn build_example(name: &str, board: Board, arch: Arch, wifi: bool) -> ExitCode {
    let workspace_root = workspace_root();
    let target = arch.target(board);
    let features = build_features(board, arch, wifi);
    println!(
        "{}",
        format!("Building example '{name}' with features: {features}").cyan()
    );

    if run_command(Command::new("cargo").current_dir(&workspace_root).args([
        "build",
        "--example",
        name,
        "--target",
        target,
        "--features",
        &features,
        "--no-default-features",
    ])) {
        println!("{}", "Build successful! ✨".green());
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn build_uf2(name: &str, board: Board, arch: Arch, wifi: bool) -> ExitCode {
    let workspace_root = workspace_root();
    let target = arch.target(board);
    let features = build_features(board, arch, wifi);

    println!(
        "{}",
        format!("Building UF2 for example '{name}' ({board}/{arch})").cyan()
    );
    println!("  Features: {}", features.bright_black());
    println!("  Target: {}", target.bright_black());

    // Build in release mode for UF2
    if !run_command(Command::new("cargo").current_dir(&workspace_root).args([
        "build",
        "--example",
        name,
        "--release",
        "--target",
        target,
        "--features",
        &features,
        "--no-default-features",
    ])) {
        return ExitCode::FAILURE;
    }

    // Convert to UF2 using elf2uf2-rs
    let elf_path = format!("target/{target}/release/examples/{name}");
    let uf2_path = format!("{name}.uf2");

    println!("\n{}", "Converting to UF2 format...".cyan());

    if run_command(
        Command::new("elf2uf2-rs")
            .current_dir(&workspace_root)
            .args([&elf_path, &uf2_path]),
    ) {
        println!("{}", format!("UF2 created: {uf2_path} 🚀").green().bold());
        println!("{}", "Ready to drag-and-drop to your Pico!".bright_black());
        ExitCode::SUCCESS
    } else {
        println!(
            "{}",
            "Note: Install elf2uf2-rs with: cargo install elf2uf2-rs".yellow()
        );
        ExitCode::FAILURE
    }
}

#[derive(Debug, Clone)]
struct ExampleInfo {
    name: String,
    wifi_required: bool,
}

fn discover_examples(workspace_root: &Path) -> Vec<ExampleInfo> {
    let examples_dir = workspace_root.join("examples");
    let mut examples = Vec::new();
    for entry in fs::read_dir(&examples_dir).expect("Failed to read examples directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("Example file must have valid UTF-8 name")
            .to_string();
        let source = fs::read_to_string(&path).expect("Failed to read example source");
        if source.contains("check-all: skip") {
            println!(
                "{}",
                format!("Skipping example '{}' (opt-out)", name).bright_black()
            );
            continue;
        }
        let wifi_required = source.contains("#![cfg(feature = \"wifi\")]");
        examples.push(ExampleInfo {
            name,
            wifi_required,
        });
    }
    examples.sort_by(|a, b| a.name.cmp(&b.name));
    examples
}

fn build_features(board: Board, arch: Arch, wifi: bool) -> String {
    let mut features = vec![board.to_string(), arch.to_string()];
    if wifi {
        features.push("wifi".to_string());
    }
    features.join(",")
}

fn workspace_root() -> PathBuf {
    // The xtask binary is in target/x86_64-pc-windows-msvc/debug/ or similar
    // We need to find the workspace root (parent of xtask directory)
    std::env::current_dir().expect("Failed to get current directory")
}

fn host_target() -> Option<String> {
    let output = Command::new("rustc").arg("-vV").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(host) = line.strip_prefix("host: ") {
            return Some(host.trim().to_string());
        }
    }
    None
}

fn run_command(cmd: &mut Command) -> bool {
    match cmd.status() {
        Ok(status) => status.success(),
        Err(e) => {
            eprintln!("{}", format!("Failed to execute command: {e}").red());
            false
        }
    }
}
