use anyhow::{Context, Result};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use meridian_config::{load_workflow, WorkflowWatcher};
use meridian_orchestrator::Orchestrator;
use meridian_tracker::{GithubTracker, SqliteTracker, Tracker};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "meridian", version, about = "Coding-agent orchestrator")]
struct Cli {
    /// Path to WORKFLOW.md (defaults to ./WORKFLOW.md).
    #[arg(long, env = "MERIDIAN_WORKFLOW")]
    workflow: Option<PathBuf>,

    /// HTTP/WS server port (overrides server.port from WORKFLOW.md).
    #[arg(long, env = "MERIDIAN_PORT")]
    port: Option<u16>,

    /// Bind address (default 127.0.0.1).
    #[arg(long, env = "MERIDIAN_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Static renderer asset directory (defaults to ./desktop/dist-renderer if present).
    #[arg(long, env = "MERIDIAN_STATIC_DIR")]
    static_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let wf_path = cli
        .workflow
        .unwrap_or_else(|| PathBuf::from("WORKFLOW.md"));
    info!(path = %wf_path.display(), "loading workflow");
    let initial = load_workflow(&wf_path)
        .await
        .with_context(|| format!("failed to load workflow at {}", wf_path.display()))?;

    initial.config.preflight().with_context(|| "startup preflight")?;

    let watcher = WorkflowWatcher::start(&wf_path, initial.clone()).await?;
    let workflow = watcher.handle.clone();
    // Keep watcher alive for the whole process.
    Box::leak(Box::new(watcher));

    let tracker: Arc<dyn Tracker> = match initial.config.tracker.kind.to_lowercase().as_str() {
        "github" => Arc::new(GithubTracker::from_config(&initial.config.tracker)?),
        "sqlite" => {
            let db_path = initial.config.effective_db_path();
            info!(path = %db_path.display(), "opening sqlite store");
            Arc::new(SqliteTracker::open(&db_path).await?)
        }
        other => anyhow::bail!("unsupported tracker.kind: {other}"),
    };

    let orch = Orchestrator::new(tracker, workflow.clone());
    let handle = orch.handle();

    // Start HTTP server.
    let port = cli.port.unwrap_or(initial.config.server.port);
    let addr: SocketAddr = format!("{}:{}", cli.host, port).parse()?;
    let static_dir = cli.static_dir.or_else(|| {
        let p = PathBuf::from("desktop/dist-renderer");
        p.is_dir().then_some(p)
    });

    let server_handle = handle.clone();
    let server_workflow = workflow.clone();
    tokio::spawn(async move {
        if let Err(e) = meridian_server::serve(addr, server_handle, server_workflow, static_dir).await {
            tracing::error!(error = %e, "http server crashed");
        }
    });

    // Run orchestrator forever.
    orch.run().await;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
}
