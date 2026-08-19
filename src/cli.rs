use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "shotgun", version, about = "OpenAPI-to-OpenAPI translation reverse proxy")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Diff two OpenAPI specs and generate a mapping file.
    Init {
        /// Path or URL to the source OpenAPI spec (the API you want to expose).
        #[arg(long)]
        source: String,
        /// Path or URL to the target OpenAPI spec (the upstream API).
        #[arg(long)]
        target: String,
        /// Where to write the mapping file.
        #[arg(long, default_value = "mappings.toml")]
        output: String,
        /// Output format: toml, yaml, json.
        #[arg(long, default_value = "toml")]
        format: String,
    },
    /// Run the translation proxy using a mapping file.
    Serve {
        /// Path to the mapping file.
        #[arg(long, default_value = "mappings.toml")]
        mappings: String,
        /// Base URL of the upstream/target API server.
        #[arg(long)]
        target_url: String,
        /// Listen address.
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: String,
        /// Log level.
        #[arg(long, default_value = "info")]
        log_level: String,
        /// Log requests to unmapped endpoints.
        #[arg(long, default_value_t = false)]
        log_unmapped: bool,
    },
    /// Re-diff updated specs and merge into an existing mapping file.
    Sync {
        /// Updated source spec.
        #[arg(long)]
        source: String,
        /// Updated target spec.
        #[arg(long)]
        target: String,
        /// Existing mapping file to merge with.
        #[arg(long, default_value = "mappings.toml")]
        mappings: String,
    },
    /// Check a mapping file for errors, warnings, and coverage stats.
    Validate {
        /// Path to the mapping file.
        #[arg(long, default_value = "mappings.toml")]
        mappings: String,
    },
}
