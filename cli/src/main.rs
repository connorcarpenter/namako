//! Namako CLI — NPAP v1 Commands
//!
//! This binary provides the core NPAP workflow commands:
//! - `namako lint` — Parse features, resolve against adapter manifest, emit resolved_plan.json
//! - `namako run` — Execute resolved plan via adapter, emit run_report.json
//! - `namako verify` — Recompute from sources, compare to baseline certification
//! - `namako update-cert` — Update certification baseline (manual, with refusal rules)
//! - `namako status` — Deterministic JSON status for Tesaki FSM (v2)
//! - `namako review` — Work backlog packet for Tesaki (v2)
//! - `namako explain` — Scenario fidelity packet for Tesaki (v2)

use anyhow::Result;
use clap::{Parser, Subcommand};

mod explain;
mod lint;
mod review;
mod status;
mod update_cert;
mod verify;

/// Namako NPAP v1 CLI
#[derive(Parser, Debug)]
#[command(name = "namako")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Parse features and resolve steps against adapter manifest.
    /// Emits resolved_plan.json.
    Lint(lint::LintArgs),
    /// Verify run report against current sources and certification baseline.
    Verify(verify::VerifyArgs),
    /// Update certification baseline (requires all tests passing).
    UpdateCert(update_cert::UpdateCertArgs),
    /// Get current FSM state and identity hashes (v2 Tesaki enablement).
    Status(status::StatusArgs),
    /// Generate work backlog packet for Tesaki (v2).
    Review(review::ReviewArgs),
    /// Generate scenario fidelity packet for Tesaki (v2).
    Explain(explain::ExplainArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Lint(args) => lint::run(args),
        Commands::Verify(args) => verify::run(args),
        Commands::UpdateCert(args) => update_cert::run(args),
        Commands::Status(args) => status::run(args),
        Commands::Review(args) => review::run(args),
        Commands::Explain(args) => explain::run(args),
    }
}
