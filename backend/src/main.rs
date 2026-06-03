#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Explicit subcommand dispatch (no clap — one subcommand doesn't justify a
    // CLI-parser dependency). parse_command rejects unknown tokens rather than
    // falling through to the server (see its docs); anyhow::Err exits non-zero
    // via #[tokio::main]. eprintln!/println! are forbidden (see backend/CLAUDE.md).
    match reverie_api::parse_command(std::env::args().nth(1).as_deref())? {
        reverie_api::Command::Migrate => reverie_api::run_migrate().await,
        reverie_api::Command::Serve => reverie_api::run().await,
    }
}
