use anyhow::{bail, Result};
use chrono::Local;
use clap::Parser;
use dotenv::dotenv;
use generate_image::{download_image, generate_image};
use once_cell::sync::Lazy;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Mutex;
use std::{fs, thread};
use toml::Value;

mod app;
mod convert_image_to_ascii;
mod generate_image;
mod util;

// Path to the directory containing the images to draw
static PATHS: Lazy<Mutex<Vec<PathBuf>>> = Lazy::new(|| Mutex::new(Vec::new()));

static APP_EXIT: AtomicBool = AtomicBool::new(false);
static GEN_EXIT: AtomicBool = AtomicBool::new(false);

// Maximum number of images to generates
const MAX_IMAGES: u8 = 5;

/// You can use sample themes for tnap and generate image with default prompts or your own prompts.
#[derive(Parser)]
#[command(version, about, long_about = None)] // Read from Cargo.toml
struct Args {
    /// Use the sample theme without generating images
    #[arg(short, long)]
    theme: Option<String>,

    /// Generate Image by looking up the corresponding value in config.toml
    /// using the subsequent string as a key and using it as a prompt.
    #[arg(short, long)]
    key: Option<String>,

    /// Generate images with user-considered prompt
    #[arg(short, long)]
    prompt: Option<String>,

    /// Convert an image to ASCII art
    #[arg(short, long)]
    ascii: bool,
}

fn main() -> Result<()> {
    dotenv().ok(); // Read environment variable from .env file
    env_logger::init();

    let args = Args::parse();
    match (args.theme, args.key, args.prompt) {
        (Some(theme), None, None) => display_theme(&theme, args.ascii),
        (None, Some(key), None) => {
            let prompt = read_config(&key)?;
            display_generated_image(&prompt, args.ascii)
        }
        (None, None, Some(prompt)) => display_generated_image(&prompt, args.ascii),
        // TODO: Set default values
        (None, None, None) => display_theme("cat", args.ascii),
        _ => bail!("Invalid arguments combination."),
    }
}

fn read_config(key: &str) -> Result<String> {
    // TODO: Use an environment variable
    let contents = fs::read_to_string("./config.toml").expect("config.toml does not exist.");
    let value = contents.parse::<Value>().unwrap();

    match value
        .get("prompts")
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
    {
        Some(prompt) => Ok(prompt.to_string()),
        None => bail!("Key not found in config."),
    }
}

fn display_theme(theme: &str, ascii: bool) -> Result<()> {
    // TODO: Use an environment variable
    let path = Path::new("./themes")
        .join(theme)
        .join(format!("{}_01.png", theme));

    // Check if the theme exists and has images
    if path.exists() {
        let dir = path.parent().unwrap();
        log::info!("{:?}", fs::canonicalize(dir));

        return app::run(dir, ascii);
    }
    bail!("Theme '{}' not found.", theme);
}

fn display_generated_image(prompt: &str, ascii: bool) -> Result<()> {
    // TODO: Use an environment variable
    let time = Local::now().format("%Y_%m%d_%H%M").to_string();
    let dir_path = Path::new("./generated_images").join(time);
    create_dir_all(&dir_path)?;

    // TODO: Use an environment variable
    // Add an image path to display while waiting for image generation
    let path_to_sample = Path::new("./examples").join("girl_with_headphone.png");
    PATHS.lock().unwrap().push(path_to_sample);

    let dir = dir_path.clone();
    let prompt = prompt.to_string();
    let handle = thread::spawn(move || {
        let mut url = generate_image(&prompt).unwrap();
        let mut path = dir.join("0.png");
        download_image(&url, &path).expect("Failed to download a generated image.");

        PATHS.lock().unwrap().push(path);
        PATHS.lock().unwrap().remove(0); // Remove a sample image path

        for i in 1..MAX_IMAGES {
            if APP_EXIT.load(SeqCst) {
                break;
            }

            url = generate_image(&prompt).unwrap();
            path = dir.join(&format!("{}.png", i));
            download_image(&url, &path).expect("Failed to download a generated image.");

            PATHS.lock().unwrap().push(path);
        }

        GEN_EXIT.store(true, SeqCst);
    });

    app::run(&dir_path, ascii)?;

    if !GEN_EXIT.load(SeqCst) {
        eprintln!("Waiting for image generation to complete...");
    }

    handle
        .join()
        .expect("Couldn't join on the associated thread.");

    Ok(())
}
