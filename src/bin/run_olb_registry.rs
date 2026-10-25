//! Host runner for the olb_registry LEZ program (Phase 0 task 6).
//!
//! `anchor` submits a Merkle root over an OLB record stream, plus the leaf
//! count, to the registry PDA. `show` reads the anchored state back.
//!
//! Submission goes through the generated client (`lgs build client`), which
//! owns nonce lookup, signing-key retrieval, witness-set construction and the
//! sequencer call.

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand};
use olb_methods::OLB_REGISTRY_ELF;
use wallet::WalletCore;

#[path = "../lib.rs"]
mod scaffold_lib;
use scaffold_lib::generated::olb_registry_client::{
    AnchorAccounts, OlbRegistryClient, compute_registry_pda,
};
use scaffold_lib::runner_support::{load_program, parse_account_id};

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long)]
    program_path: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Anchor a 32-byte Merkle root and its leaf count on the sequencer.
    Anchor {
        /// Signing authority account id (a self-owned public account).
        #[arg(long)]
        authority: String,
        /// Merkle root as 64 hex characters.
        #[arg(long)]
        root: String,
        /// Number of leaves the root covers.
        #[arg(long)]
        count: u64,
    },
    /// Print the registry PDA for this program.
    Pda,
}

fn parse_root(raw: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = hex::decode(raw.trim_start_matches("0x")).context("root must be hex")?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| anyhow!("root must be exactly 32 bytes (64 hex chars), got {}", bytes.len()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let wallet_core =
        WalletCore::from_env().context("failed to initialize wallet from environment")?;
    let program = load_program(cli.program_path.as_deref(), OLB_REGISTRY_ELF, "olb_registry");
    let program_id = program.id();

    match cli.command {
        Command::Pda => {
            println!("{}", compute_registry_pda(&program_id));
        }

        Command::Anchor {
            authority,
            root,
            count,
        } => {
            let authority_id = parse_account_id(&authority);
            let root_bytes = parse_root(&root)?;
            // The registry account is derived, not supplied: its seed is fixed
            // in the program (`pda = literal("olb_registry")`), so accepting
            // one from the caller would only let them pass the wrong account.
            let registry = compute_registry_pda(&program_id);

            let client = OlbRegistryClient::new(&wallet_core, program_id);
            let tx_hash = client
                .anchor(
                    AnchorAccounts {
                        registry,
                        authority: authority_id,
                    },
                    root_bytes,
                    count,
                )
                .await
                .map_err(|e| anyhow!("anchor submission failed: {e}"))?;

            println!("submitted transaction: tx_hash={tx_hash}");
            println!("registry account: {registry}");
            println!("anchored count={count} root={}", hex::encode(root_bytes));
            println!(
                "verification hint: lgs wallet -- account get --account-id Public/{registry}"
            );
        }
    }

    Ok(())
}
