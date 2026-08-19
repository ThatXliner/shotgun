mod cli;

use clap::Parser;
use cli::{Cli, Command};
use shotgun::{config, mapping, proxy};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init {
            source,
            target,
            output,
            format,
        } => {
            tracing_subscriber::fmt::init();
            config::run_init(&source, &target, &output, &format)?;
        }
        Command::Serve {
            mappings,
            target_url,
            listen,
            log_level,
            log_unmapped,
        } => {
            init_tracing(&log_level);
            let mf = mapping::reader::read_mapping_file(&mappings)?;
            let target_url = url::Url::parse(&target_url)?;
            let listen: std::net::SocketAddr = listen.parse()?;
            proxy::serve(mf, target_url, listen, log_unmapped).await?;
        }
        Command::Sync {
            source,
            target,
            mappings,
        } => {
            tracing_subscriber::fmt::init();
            config::run_sync(&source, &target, &mappings)?;
        }
        Command::Validate { mappings } => {
            tracing_subscriber::fmt::init();
            config::run_validate(&mappings)?;
        }
    }

    Ok(())
}

fn init_tracing(log_level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_new(log_level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
