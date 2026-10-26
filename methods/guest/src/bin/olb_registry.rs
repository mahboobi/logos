#![no_main]

use nssa_core::account::Data;
use spel_framework::prelude::*;

#[cfg(not(test))]
risc0_zkvm::guest::entry!(main);

// Keep this body byte-for-byte in step with the copy in src/lib.rs: that copy
// is what `lgs build idl` extracts the IDL from, this one is what the
// sequencer executes, and they must describe the same program.
#[lez_program]
mod olb_registry {
    #[allow(unused_imports)]
    use super::*;

    /// Anchor a Merkle root for one project's record stream.
    ///
    /// The registry account is derived per project: the PDA seeds are the
    /// constant "olb_registry" plus the project tag argument, so two projects
    /// — and two nodes anchoring different projects — never collide. The
    /// previous `literal("olb_registry")` seed resolved to ONE account for
    /// every project on every node, so concurrent anchors overwrote each
    /// other (docs/05 §4 always specified a project tag; the first cut did
    /// not implement it).
    ///
    /// `root` is the 32-byte Merkle root over the stream's commits and
    /// `count` is how many leaves it covers. The pair is stored in the
    /// account's `data`, never its `balance`: LEZ enforces conservation of
    /// total balance across a transaction, so writing to `balance` mints
    /// tokens and the execution check rejects it.
    ///
    /// Layout: [0..8) count LE u64, [8..40) root. Fixed width so a reader
    /// never has to parse.
    #[instruction]
    pub fn anchor(
        #[account(init, pda = [r#const("olb_registry"), arg("project_tag")])]
        mut registry: AccountWithMetadata,
        #[account(signer)]
        authority: AccountWithMetadata,
        project_tag: String,
        root: [u8; 32],
        count: u64,
    ) -> SpelResult {
        let mut buf = Vec::with_capacity(40);
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&root);

        registry.account.data = Data::try_from(buf).map_err(|_| {
            SpelError::SerializationError {
                message: "registry record must be 40 bytes (u64 count ++ 32-byte root)"
                    .to_string(),
            }
        })?;

        Ok(SpelOutput::execute(vec![registry, authority], vec![]))
    }
}
