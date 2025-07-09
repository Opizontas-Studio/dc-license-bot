use std::path::PathBuf;

use clap::Parser;

pub mod commands;
pub mod config;
pub mod database;
pub mod error;
pub mod handlers;
mod services;
pub mod utils;
pub mod types;

#[derive(Parser)]
pub struct Args {
    #[clap(short, long, default_value = "config.toml")]
    /// Path to the configuration file
    pub config: PathBuf,
    /// Path to the database file
    #[clap(short, long, default_value = "./data/bot.db")]
    pub db: PathBuf,
    /// Path to the default licenses file
    #[clap(short, long, default_value = "./system_licenses.json")]
    pub default_licenses: PathBuf,
}
