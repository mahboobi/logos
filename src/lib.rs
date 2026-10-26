pub mod generated;

#[allow(dead_code)]
pub mod runner_support {
    use nssa::{AccountId, program::Program};

    pub fn parse_account_id(raw: &str) -> AccountId {
        let normalized = raw
            .strip_prefix("Public/")
            .or_else(|| raw.strip_prefix("Private/"))
            .unwrap_or(raw);

        normalized
            .parse()
            .unwrap_or_else(|err| panic!("invalid account_id `{raw}`: {err}"))
    }

    pub fn load_program(program_path: Option<&str>, embedded_elf: &[u8], label: &str) -> Program {
        let bytes = if let Some(path) = program_path {
            std::fs::read(path)
                .unwrap_or_else(|err| panic!("failed to read {label} binary at `{path}`: {err}"))
        } else {
            embedded_elf.to_vec()
        };

        Program::new(bytes).unwrap_or_else(|err| panic!("failed to parse {label} program: {err}"))
    }
}

// Host-side program definition for IDL extraction and testing.
// The guest binary (methods/guest) handles zkvm execution.
use nssa_core::account::{AccountWithMetadata, Data};
use spel_framework::prelude::*;

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

#[cfg(test)]
mod tests {
    #[test]
    fn __lssa_idl_print() {
        println!("--- LSSA IDL BEGIN olb_registry ---");
        println!("{}", super::PROGRAM_IDL_JSON);
        println!("--- LSSA IDL END olb_registry ---");
    }
}
