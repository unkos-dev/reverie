#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Explicit subcommand dispatch (no clap — one subcommand doesn't justify a
    // CLI-parser dependency). parse_command rejects unknown tokens rather than
    // falling through to the server (see its docs); anyhow::Err exits non-zero
    // via #[tokio::main]. eprintln!/println! are forbidden (see backend/CLAUDE.md).
    let args: Vec<String> = std::env::args().skip(1).collect();
    match reverie_api::parse_command(&args)? {
        reverie_api::Command::Migrate => reverie_api::run_migrate().await,
        reverie_api::Command::PrintConfigSchema => reverie_api::print_config_schema(),
        reverie_api::Command::Bootstrap => reverie_api::run_bootstrap().await,
        reverie_api::Command::ResetPassword { email } => {
            reverie_api::run_reset_password(&email).await
        }
        reverie_api::Command::UnlockAccount { email } => {
            reverie_api::run_unlock_account(&email).await
        }
        reverie_api::Command::Serve => reverie_api::run().await,
    }
}
