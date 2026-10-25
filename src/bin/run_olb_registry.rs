//! Host runner for the olb_registry LEZ program (Phase 0 task 6).
//!
//! `anchor` takes the Merkle root over an OLB record stream and the leaf count
//! and writes them to the registry PDA.

use clap::{Parser, Subcommand};
use olb_methods::OLB_REGISTRY_ELF;
use wallet::WalletCore;

#[path = "../lib.rs"]
mod scaffold_lib;
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
    /// Anchor a 32-byte Merkle root and its leaf count.
    Anchor {
        /// Registry PDA account id.
        #[arg(long)]
        registry: String,
        /// Signing authority account id.
        #[arg(long)]
        authority: String,
        /// Merkle root as 64 hex characters.
        #[arg(long)]
        root: String,
        /// Number of leaves the root covers.
        #[arg(long)]
        count: u64,
    },
}

fn parse_root(raw: &str) -> [u8; 32] {
    let bytes = hex::decode(raw.trim_start_matches("0x"))
        .unwrap_or_else(|e| panic!("root must be hex: {e}"));
    <[u8; 32]>::try_from(bytes.as_slice())
        .unwrap_or_else(|_| panic!("root must be exactly 32 bytes (64 hex chars)"))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let _wallet_core = WalletCore::from_env().expect("wallet should initialize from environment");
    let _program = load_program(cli.program_path.as_deref(), OLB_REGISTRY_ELF, "olb_registry");

    match cli.command {
        Command::Anchor {
            registry,
            authority,
            root,
            count,
        } => {
            let _registry_id = parse_account_id(&registry);
            let _authority_id = parse_account_id(&authority);
            let root_bytes = parse_root(&root);

            println!(
                "anchor registry={registry} authority={authority} count={count} root={}",
                hex::encode(root_bytes)
            );
            // NOT YET SUBMITTED. The lez-framework template ships its runner
            // with this same gap (`// TODO: submit transaction via wallet` in
            // run_lez_counter.rs), so nothing reaches the sequencer yet and
            // docs/07 Phase 0's "anchor transaction visible on the local
            // sequencer" is not met by this binary alone. Tracked as docs/08
            // R12 rather than papered over with a fake success line.
            eprintln!(
                "warning: transaction submission is not implemented — \
                 nothing was sent to the sequencer (docs/08 R12)"
            );
        }
    }
}
