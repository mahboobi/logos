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

    /// Anchor a Merkle root for one OLB record stream.
    ///
    /// `root` is the 32-byte Merkle root over the stream's commits and `count`
    /// is how many leaves it covers. The pair is stored in the registry
    /// account's `data`, never its `balance`: LEZ enforces conservation of
    /// total balance across a transaction, so writing to `balance` mints
    /// tokens and the execution check rejects it with MismatchedTotalBalance.
    ///
    /// Layout: [0..8) count LE u64, [8..40) root. Fixed width so a reader
    /// never has to parse.
    #[instruction]
    pub fn anchor(
        #[account(init, pda = literal("olb_registry"))]
        mut registry: AccountWithMetadata,
        #[account(signer)]
        authority: AccountWithMetadata,
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
