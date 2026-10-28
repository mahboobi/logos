//! The project-level data model, v2 (docs/13, `schemas/olb-model.schema.json`).
//!
//! Replaces the idea-level model: one project is one research line with one
//! stage, and the stage is **derived from signed `stage_transition` records**
//! rather than stored. Three rules from §1 of the model hold everywhere here:
//!
//! 1. One envelope, many payloads — every record shares `Envelope`, and the
//!    type-specific content lives in `fields`.
//! 2. Append-only — an edit is a new version; nothing is overwritten.
//! 3. Derive, do not duplicate — anything computable is never stored. That is
//!    why `Project` has no `stage` field: see [`stage_of`].
//!
//! **This is v2 and does not replace v1 on disk.** Records already written
//! cannot satisfy this envelope (no record-level `sig`, and extra fields the
//! schema forbids), and CLAUDE.md is explicit that nothing in history is ever
//! rewritten. So v1 records stay exactly as they are and stay readable; v2 is
//! what gets created from here on. [`crate::types::Record`] is v1.

use serde::{Deserialize, Serialize};

/// The six stages. Lowercase on the wire, per the schema's `stage` enum —
/// note `Published`, not "Publish": the stage names a finished state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Idea,
    Hypothesis,
    Funding,
    Experiments,
    Data,
    Published,
}

impl Stage {
    pub const ALL: [Stage; 6] = [
        Stage::Idea,
        Stage::Hypothesis,
        Stage::Funding,
        Stage::Experiments,
        Stage::Data,
        Stage::Published,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Idea => "idea",
            Stage::Hypothesis => "hypothesis",
            Stage::Funding => "funding",
            Stage::Experiments => "experiments",
            Stage::Data => "data",
            Stage::Published => "published",
        }
    }

    /// Accepts the schema spelling, and the Title-case and alias spellings the
    /// older UI used ("Publish", "Experiment", "Data/Results"), so a v1 value
    /// reaching this layer is understood rather than silently dropped.
    pub fn parse(s: &str) -> Option<Stage> {
        match s.trim().to_ascii_lowercase().as_str() {
            "idea" => Some(Stage::Idea),
            "hypothesis" => Some(Stage::Hypothesis),
            "funding" => Some(Stage::Funding),
            "experiment" | "experiments" => Some(Stage::Experiments),
            "data" | "data/results" | "results" => Some(Stage::Data),
            "publish" | "published" => Some(Stage::Published),
            _ => None,
        }
    }

    /// Human label for the UI. The model's own prose uses Title case.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Idea => "Idea",
            Stage::Hypothesis => "Hypothesis",
            Stage::Funding => "Funding",
            Stage::Experiments => "Experiments",
            Stage::Data => "Data",
            Stage::Published => "Published",
        }
    }
}

/// Project roles (§2). Only an admin moves stages or publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
    Reviewer,
}

/// Why a transition happened. `start` is the project's own creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Start,
    Forward,
    Skip,
    Back,
}

/// Who acted. `kind` is required, and an agent must name the human who runs
/// it — an unattributable agent action is the thing this prevents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub key: String,
    pub kind: ActorKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    Member,
    External,
    Agent,
}

/// One guard result, recorded on the transition so a move can be audited
/// later without re-deriving what was true at the time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub rule: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The payload of a `stage_transition` record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageTransition {
    /// `None` only for the project's own creation.
    pub from: Option<Stage>,
    pub to: Stage,
    pub direction: Direction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// `override` is a Rust keyword, so the field is spelled with a trailing
    /// underscore and renamed on the wire. Without the rename it would
    /// serialise as "override_", which the schema rejects outright
    /// (`additionalProperties: false`).
    #[serde(rename = "override", default, skip_serializing_if = "Option::is_none")]
    pub override_: Option<bool>,
    pub checks: Vec<Check>,
}

/// The 16 record types (schema `record.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordType {
    Project,
    Membership,
    JoinRequest,
    StageTransition,
    IdeaContribution,
    Hypothesis,
    GrantRequest,
    Pledge,
    MilestoneDecision,
    CollectionProtocol,
    Submission,
    Review,
    DatasetSnapshot,
    Analysis,
    Manuscript,
    Publication,
}

/// The shared envelope. `sig` is on the RECORD here, which is the structural
/// difference from v1 (where only the commit carried a signature).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: RecordType,
    pub version: u64,
    pub project: String,
    pub authors: Vec<Actor>,
    pub parents: Vec<String>,
    pub created_at: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub sig: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub co_sigs: Vec<String>,
}

/// Project privacy, chosen once and immutable after version 1 (§2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privacy {
    Open,
    Private,
    InviteOnly,
}

/// The licences the model allows. A closed list, not free text, because a
/// bundle's licence has to be machine-checkable at publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Licence {
    #[serde(rename = "CC-BY-4.0")]
    CcBy40,
    #[serde(rename = "CC0-1.0")]
    Cc010,
    #[serde(rename = "CC-BY-SA-4.0")]
    CcBySa40,
}

/// How often commits are anchored on LEZ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchoring {
    Every,
    Batched,
    Daily,
    Milestones,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Duration {
    #[serde(rename = "lt_6m")]
    LessThan6Months,
    #[serde(rename = "6_12m")]
    SixToTwelveMonths,
    #[serde(rename = "1_2y")]
    OneToTwoYears,
    #[serde(rename = "gt_2y")]
    OverTwoYears,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamSize {
    #[serde(rename = "1")]
    One,
    #[serde(rename = "2_5")]
    TwoToFive,
    #[serde(rename = "6_15")]
    SixToFifteen,
    #[serde(rename = "gt_15")]
    OverFifteen,
}

/// Project settings. Only `anchoring` is required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub anchoring: Anchoring,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_approvals: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replica: Option<bool>,
    /// Open projects only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_visitors: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_tags: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewers_can_verify: Option<bool>,
}

/// A rich-text document. `blocks` stay loosely typed here: the block grammar
/// is large (§8 names the editor as the biggest UI risk), and the schema is
/// the thing that checks it. Keeping them as values means this crate does not
/// have to re-state a grammar it cannot enforce better than the contract can.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichDoc {
    pub format: RichFormat,
    pub blocks: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crdt_state: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RichFormat {
    #[serde(rename = "olb-blocks/1")]
    OlbBlocks1,
}

/// A run of text inside a block. Only `text` is required, and the schema
/// forbids anything else — notably there is no `"type": "text"` tag, which
/// is the obvious thing to add and is rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inline {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention: Option<String>,
}

impl Inline {
    pub fn text(t: &str) -> Inline {
        Inline { text: t.to_string(), marks: vec![], href: None, mention: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Bold,
    Italic,
    Code,
    Sup,
    Sub,
    Strike,
}

impl RichDoc {
    /// A one-paragraph document, which is what most text from a plain field is.
    pub fn paragraph(text: &str) -> RichDoc {
        RichDoc {
            format: RichFormat::OlbBlocks1,
            blocks: vec![serde_json::json!({
                "type": "paragraph",
                "content": [Inline::text(text)],
            })],
            crdt_state: None,
        }
    }

    pub fn empty() -> RichDoc {
        RichDoc { format: RichFormat::OlbBlocks1, blocks: vec![], crdt_state: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub identifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceKind {
    Doi,
    Url,
    Isbn,
    Olb,
}

/// The payload of a `project` record. Note what is NOT here: no `stage`, and
/// no `ideas`. The stage is derived from transitions (§1 rule 3), and work
/// lives in its own records rather than nested inside the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFields {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<RichDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<Licence>,
    pub privacy: Privacy,
    pub stages_enabled: Vec<Stage>,
    pub settings: Settings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_duration: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_size: Option<TeamSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_grant: Option<String>,
}

/// Why a proposed `stages_enabled` is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagesInvalid {
    Empty,
    /// The schema says "in lifecycle order".
    OutOfOrder,
    Duplicate(Stage),
    /// The schema says it "must end with 'published'".
    DoesNotEndAtPublished,
}

/// Check a `stages_enabled` list against the schema's three stated rules.
///
/// The schema expresses uniqueness and non-emptiness but states the ordering
/// and the final stage only in prose, so they are checked here — a project
/// whose stages ran backwards would validate and then behave absurdly.
pub fn validate_stages(stages: &[Stage]) -> Result<(), StagesInvalid> {
    if stages.is_empty() {
        return Err(StagesInvalid::Empty);
    }
    for (i, s) in stages.iter().enumerate() {
        if stages[..i].contains(s) {
            return Err(StagesInvalid::Duplicate(*s));
        }
        if i > 0 && *s <= stages[i - 1] {
            return Err(StagesInvalid::OutOfOrder);
        }
    }
    if stages.last() != Some(&Stage::Published) {
        return Err(StagesInvalid::DoesNotEndAtPublished);
    }
    Ok(())
}

/// The payload of an `idea_contribution`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdeaContributionFields {
    pub kind: IdeaKind,
    pub body: RichDoc,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Reference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeaKind {
    Proposal,
    Comment,
    Question,
    Evidence,
}

/// The payload of a `membership`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipFields {
    pub member: String,
    pub name: String,
    pub role: Role,
    pub status: MemberStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invited_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite_kind: Option<InviteKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_version: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberStatus {
    Invited,
    Active,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteKind {
    ById,
    ByCode,
    JoinRequest,
    Creator,
}

/// One entry in a project's history (§6). Every write carries one, so the
/// timeline is a projection of signed commits rather than a table anyone can
/// edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEvent {
    pub commit: String,
    pub at: String,
    pub actor: Actor,
    pub action: Action,
    pub stage: Stage,
    pub target: Target,
    /// Which protocol the work went through, when it went through one. This
    /// is what makes "who contributed where" answerable per protocol (§6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub anchor: Anchor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub state: AnchorState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorState {
    Pending,
    Anchored,
}

impl Anchor {
    /// Every event starts unanchored. Claiming otherwise before a receipt is
    /// exactly the mistake R12 recorded: a tx_hash is not a receipt.
    pub fn pending() -> Anchor {
        Anchor { state: AnchorState::Pending, batch: None }
    }
}

/// What happened, from the schema's closed list of 32 actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    ProjectCreated,
    SettingsChanged,
    MemberInvited,
    MemberJoined,
    MemberRoleChanged,
    MemberRemoved,
    JoinRequested,
    JoinDecided,
    StageMoved,
    IdeaContributed,
    HypothesisEdited,
    HypothesisReviewed,
    HypothesisCommitted,
    HypothesisRevealed,
    GrantDrafted,
    GrantOpened,
    PledgeMade,
    MilestoneDecided,
    MilestoneReleased,
    GrantClosed,
    ProtocolDrafted,
    ProtocolPublished,
    ProtocolClosed,
    SubmissionAdded,
    SubmissionCorrected,
    SubmissionReviewed,
    SnapshotFrozen,
    AnalysisEdited,
    ManuscriptEdited,
    ManuscriptReady,
    PaperPublished,
    CreditResponded,
}

// ---------------------------------------------------------------------------
// Sealing a v2 record
// ---------------------------------------------------------------------------

/// Domain tag for a v2 record preimage.
///
/// Distinct from `hashing::RECORD` (v1) on purpose: the two formats must not
/// be able to produce the same hash for the same bytes, or a v1 record could
/// be presented as a v2 one.
pub const RECORD_V2: &[u8] = b"olb.record.v2\0";

/// The bytes a v2 record's id hashes and its signature covers.
///
/// Computed over the envelope WITHOUT `id`, `sig` or `co_sigs` — none of
/// which exist yet when the preimage is formed, and all of which would make
/// the hash depend on itself.
fn record_preimage(
    kind: RecordType,
    project: &str,
    version: u64,
    authors: &[Actor],
    parents: &[String],
    created_at: &str,
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<u8>, crate::VcsError> {
    let body = serde_json::json!({
        "type": kind,
        "version": version,
        "project": project,
        "authors": authors,
        "parents": parents,
        "created_at": created_at,
        "fields": fields,
    });
    crate::hashing::preimage(RECORD_V2, &body)
}

/// Build a signed v2 record.
///
/// The id and the signature cover the same preimage, so a record cannot be
/// edited without either breaking its signature or becoming a different
/// record. Version 1 establishes the object id; later versions keep it, which
/// is why `id` is passed in rather than recomputed (docs/10 §3.1 — an object
/// id is the hash of the FIRST version, and deriving it from a later one is
/// always a bug).
pub fn seal_record(
    signing: &ed25519_dalek::SigningKey,
    kind: RecordType,
    project: &str,
    version: u64,
    id: Option<String>,
    authors: Vec<Actor>,
    parents: Vec<String>,
    created_at: String,
    fields: serde_json::Value,
) -> Result<Envelope, crate::VcsError> {
    use ed25519_dalek::Signer;

    let fields = fields.as_object().cloned().unwrap_or_default();
    let pre = record_preimage(
        kind, project, version, &authors, &parents, &created_at, &fields,
    )?;

    let id = match id {
        Some(existing) => existing,
        None => {
            if version != 1 {
                return Err(crate::VcsError::Malformed(format!(
                    "an object id comes from version 1, got version {version}"
                )));
            }
            // A project record's id IS the project id (the schema requires
            // `olb:proj:`), so the caller supplies it; everything else is
            // identified by the hash of its first version.
            crate::hashing::object_id_from_v1_hash(&crate::hashing::digest(&pre)).0
        }
    };

    // Signed over the preimage directly: ed25519 hashes internally, so
    // pre-hashing would buy nothing and risk a second construction.
    let sig = signing.sign(&pre);

    Ok(Envelope {
        id,
        kind,
        version,
        project: project.to_string(),
        authors,
        parents,
        created_at,
        fields,
        sig: hex::encode(sig.to_bytes()),
        co_sigs: vec![],
    })
}

/// Check a v2 record's signature against `key`.
pub fn verify_record(env: &Envelope, key: &ed25519_dalek::VerifyingKey) -> bool {
    use ed25519_dalek::Verifier;

    let Ok(pre) = record_preimage(
        env.kind,
        &env.project,
        env.version,
        &env.authors,
        &env.parents,
        &env.created_at,
        &env.fields,
    ) else {
        return false;
    };
    let Ok(raw) = hex::decode(&env.sig) else { return false };
    let Ok(bytes) = <[u8; 64]>::try_from(raw.as_slice()) else { return false };
    key.verify(&pre, &ed25519_dalek::Signature::from_bytes(&bytes)).is_ok()
}

// ---------------------------------------------------------------------------
// Stage machine (§4)
// ---------------------------------------------------------------------------

/// The stage a project is in, from its `stage_transition` records.
///
/// Derived, never stored (§1 rule 3). The latest transition wins, so the
/// records are the single source of truth and a stale cached stage cannot
/// contradict them.
pub fn stage_of(transitions: &[StageTransition]) -> Stage {
    transitions.last().map(|t| t.to).unwrap_or(Stage::Idea)
}

/// What a project must contain for the move out of `from` to be allowed.
///
/// The counts are supplied by the caller rather than read here, because this
/// crate has no store: `olb_core` knows how many contributions or committed
/// hypotheses a project holds, and this decides what that means.
#[derive(Debug, Clone, Copy, Default)]
pub struct Evidence {
    pub idea_contributions: usize,
    pub committed_hypotheses: usize,
    pub grant_funded: bool,
    pub published_protocols: usize,
    pub verified_submissions: usize,
    pub manuscript_ready: bool,
    pub all_cited_snapshots_frozen: bool,
    pub credits_settled: bool,
    pub every_file_has_two_copies: bool,
}

/// Guard rules for leaving `from`, as `(rule, passed)` pairs.
///
/// Returned whole, pass or fail, because the transition record stores every
/// check that was evaluated — not only the ones that failed.
pub fn checks_for(from: Stage, ev: &Evidence) -> Vec<Check> {
    let c = |rule: &str, passed: bool| Check {
        rule: rule.to_string(),
        passed,
        detail: None,
    };
    match from {
        Stage::Idea => vec![c("at_least_one_idea_contribution", ev.idea_contributions >= 1)],
        Stage::Hypothesis => vec![c("at_least_one_hypothesis_committed", ev.committed_hypotheses >= 1)],
        Stage::Funding => vec![c("grant_funded", ev.grant_funded)],
        Stage::Experiments => vec![
            c("at_least_one_protocol_published", ev.published_protocols >= 1),
            c("at_least_one_verified_submission", ev.verified_submissions >= 1),
        ],
        Stage::Data => vec![
            c("manuscript_ready", ev.manuscript_ready),
            c("cited_snapshots_frozen", ev.all_cited_snapshots_frozen),
            c("credits_settled", ev.credits_settled),
            c("no_file_under_two_copies", ev.every_file_has_two_copies),
        ],
        // Published is final (§4). A revision is a new publication version,
        // not a move out of the stage.
        Stage::Published => vec![c("published_is_final", false)],
    }
}

/// Why a proposed move was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Only an admin moves stages (§2, §5).
    NotAdmin,
    /// Published is final.
    PublishedIsFinal,
    /// `to` is not one of the stages this project uses.
    StageNotEnabled(Stage),
    /// A backward move needs a written reason.
    BackWithoutReason,
    /// One or more guards failed, and the move is not overridable.
    GuardsFailed(Vec<Check>),
    /// Only Funding → Experiments may be overridden, and it needs a reason.
    OverrideNotAllowed,
    /// The move goes nowhere.
    SameStage,
}

/// The next stage a project would move to, skipping stages it does not use.
pub fn next_enabled(from: Stage, enabled: &[Stage]) -> Option<Stage> {
    Stage::ALL
        .iter()
        .copied()
        .filter(|s| enabled.contains(s))
        .find(|s| *s > from)
}

/// Decide a stage move, returning the transition to sign or why it is refused.
///
/// Everything the model requires of a move is enforced here rather than in the
/// UI, so a forged or hand-built call cannot skip a gate (§5 requires this to
/// live in `olb_core`).
#[allow(clippy::too_many_arguments)]
pub fn plan_move(
    from: Stage,
    to: Stage,
    enabled: &[Stage],
    role: Role,
    ev: &Evidence,
    reason: Option<&str>,
    want_override: bool,
) -> Result<StageTransition, Refusal> {
    if role != Role::Admin {
        return Err(Refusal::NotAdmin);
    }
    if from == Stage::Published {
        return Err(Refusal::PublishedIsFinal);
    }
    if to == from {
        return Err(Refusal::SameStage);
    }
    if !enabled.contains(&to) {
        return Err(Refusal::StageNotEnabled(to));
    }

    // Backwards: allowed for an admin, but only with a written reason.
    if to < from {
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        return match reason {
            None => Err(Refusal::BackWithoutReason),
            Some(r) => Ok(StageTransition {
                from: Some(from),
                to,
                direction: Direction::Back,
                reason: Some(r.to_string()),
                override_: None,
                checks: vec![],
            }),
        };
    }

    let checks = checks_for(from, ev);
    let failed: Vec<Check> = checks.iter().filter(|c| !c.passed).cloned().collect();

    if !failed.is_empty() {
        // Only Funding → Experiments is overridable, and only with a reason.
        let overridable = from == Stage::Funding;
        let reason_given = reason.map(str::trim).filter(|r| !r.is_empty());
        if !(want_override && overridable) {
            return Err(Refusal::GuardsFailed(failed));
        }
        let Some(r) = reason_given else {
            return Err(Refusal::OverrideNotAllowed);
        };
        return Ok(StageTransition {
            from: Some(from),
            to,
            direction: Direction::Forward,
            reason: Some(r.to_string()),
            override_: Some(true),
            checks,
        });
    }

    // A move that lands past the immediately next stage skipped one the
    // project does not use, and the model records that distinctly.
    let direction = match next_enabled(from, enabled) {
        Some(n) if n == to => Direction::Forward,
        _ => Direction::Skip,
    };
    Ok(StageTransition {
        from: Some(from),
        to,
        direction,
        reason: reason.map(str::trim).filter(|r| !r.is_empty()).map(String::from),
        override_: None,
        checks,
    })
}

/// The transition that opens a project's life.
pub fn start(first: Stage) -> StageTransition {
    StageTransition {
        from: None,
        to: first,
        direction: Direction::Start,
        reason: None,
        override_: None,
        checks: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn admin() -> Role { Role::Admin }
    const ALL: [Stage; 6] = Stage::ALL;

    fn full_evidence() -> Evidence {
        Evidence {
            idea_contributions: 1,
            committed_hypotheses: 1,
            grant_funded: true,
            published_protocols: 1,
            verified_submissions: 1,
            manuscript_ready: true,
            all_cited_snapshots_frozen: true,
            credits_settled: true,
            every_file_has_two_copies: true,
        }
    }

    // ---- wire format ----

    #[test]
    fn stage_is_lowercase_and_published_not_publish() {
        assert_eq!(serde_json::to_value(Stage::Published).unwrap(), json!("published"));
        assert_eq!(serde_json::to_value(Stage::Experiments).unwrap(), json!("experiments"));
        // The old UI spelling still parses, so a v1 value is understood.
        assert_eq!(Stage::parse("Publish"), Some(Stage::Published));
        assert_eq!(Stage::parse("Data/Results"), Some(Stage::Data));
        assert_eq!(Stage::parse("nonsense"), None);
    }

    #[test]
    fn record_type_is_snake_case() {
        assert_eq!(
            serde_json::to_value(RecordType::IdeaContribution).unwrap(),
            json!("idea_contribution")
        );
        assert_eq!(
            serde_json::to_value(RecordType::DatasetSnapshot).unwrap(),
            json!("dataset_snapshot")
        );
    }

    /// The field is `override_` in Rust and must be `override` on the wire;
    /// the schema forbids unknown properties, so a missed rename is fatal.
    #[test]
    fn override_serialises_without_its_underscore() {
        let t = StageTransition {
            from: Some(Stage::Funding),
            to: Stage::Experiments,
            direction: Direction::Forward,
            reason: Some("grant withdrawn, proceeding on own funds".into()),
            override_: Some(true),
            checks: vec![],
        };
        let v = serde_json::to_value(&t).unwrap();
        assert!(v.get("override").is_some(), "got {v}");
        assert!(v.get("override_").is_none());
    }

    #[test]
    fn an_absent_override_is_omitted_not_null() {
        let v = serde_json::to_value(start(Stage::Idea)).unwrap();
        assert!(v.get("override").is_none());
        assert!(v.get("reason").is_none());
        assert_eq!(v["direction"], json!("start"));
        assert_eq!(v["from"], json!(null), "start has an explicit null from");
    }

    // ---- derivation ----

    #[test]
    fn stage_is_derived_from_the_latest_transition() {
        assert_eq!(stage_of(&[]), Stage::Idea, "a project with no moves is at Idea");
        let ts = vec![
            start(Stage::Idea),
            StageTransition { from: Some(Stage::Idea), to: Stage::Hypothesis,
                direction: Direction::Forward, reason: None, override_: None, checks: vec![] },
        ];
        assert_eq!(stage_of(&ts), Stage::Hypothesis);
    }

    // ---- authority ----

    #[test]
    fn only_an_admin_moves_a_stage() {
        for role in [Role::Member, Role::Reviewer] {
            let r = plan_move(Stage::Idea, Stage::Hypothesis, &ALL, role,
                              &full_evidence(), None, false);
            assert_eq!(r, Err(Refusal::NotAdmin), "{role:?} must not move stages");
        }
        assert!(plan_move(Stage::Idea, Stage::Hypothesis, &ALL, admin(),
                          &full_evidence(), None, false).is_ok());
    }

    #[test]
    fn published_is_final() {
        let r = plan_move(Stage::Published, Stage::Data, &ALL, admin(),
                          &full_evidence(), Some("retracting"), false);
        assert_eq!(r, Err(Refusal::PublishedIsFinal));
    }

    // ---- guards ----

    #[test]
    fn idea_needs_a_contribution_before_moving_on() {
        let ev = Evidence::default();
        let r = plan_move(Stage::Idea, Stage::Hypothesis, &ALL, admin(), &ev, None, false);
        match r {
            Err(Refusal::GuardsFailed(f)) => {
                assert_eq!(f.len(), 1);
                assert_eq!(f[0].rule, "at_least_one_idea_contribution");
            }
            other => panic!("expected a failed guard, got {other:?}"),
        }
    }

    /// Every check is recorded, not only the failures — the transition is the
    /// audit trail for what was true at the time.
    #[test]
    fn a_successful_move_records_all_its_checks() {
        let t = plan_move(Stage::Experiments, Stage::Data, &ALL, admin(),
                          &full_evidence(), None, false).unwrap();
        assert_eq!(t.checks.len(), 2);
        assert!(t.checks.iter().all(|c| c.passed));
        assert_eq!(t.direction, Direction::Forward);
    }

    #[test]
    fn data_to_published_has_four_gates() {
        let mut ev = full_evidence();
        ev.every_file_has_two_copies = false;
        let r = plan_move(Stage::Data, Stage::Published, &ALL, admin(), &ev, None, false);
        match r {
            Err(Refusal::GuardsFailed(f)) => {
                assert_eq!(f.len(), 1);
                assert_eq!(f[0].rule, "no_file_under_two_copies");
            }
            other => panic!("expected one failed gate, got {other:?}"),
        }
        assert_eq!(checks_for(Stage::Data, &full_evidence()).len(), 4);
    }

    // ---- override ----

    #[test]
    fn only_funding_may_be_overridden_and_only_with_a_reason() {
        let mut ev = full_evidence();
        ev.grant_funded = false;
        // With a reason: allowed, and marked as an override.
        let t = plan_move(Stage::Funding, Stage::Experiments, &ALL, admin(), &ev,
                          Some("self-funded"), true).unwrap();
        assert_eq!(t.override_, Some(true));
        assert_eq!(t.reason.as_deref(), Some("self-funded"));
        // Without one: refused.
        assert_eq!(
            plan_move(Stage::Funding, Stage::Experiments, &ALL, admin(), &ev, None, true),
            Err(Refusal::OverrideNotAllowed)
        );
        // A different stage cannot be overridden at all.
        let mut ev2 = full_evidence();
        ev2.idea_contributions = 0;
        assert!(matches!(
            plan_move(Stage::Idea, Stage::Hypothesis, &ALL, admin(), &ev2, Some("why"), true),
            Err(Refusal::GuardsFailed(_))
        ));
    }

    // ---- backwards ----

    #[test]
    fn a_backward_move_requires_a_written_reason() {
        assert_eq!(
            plan_move(Stage::Data, Stage::Experiments, &ALL, admin(),
                      &full_evidence(), None, false),
            Err(Refusal::BackWithoutReason)
        );
        assert_eq!(
            plan_move(Stage::Data, Stage::Experiments, &ALL, admin(),
                      &full_evidence(), Some("   "), false),
            Err(Refusal::BackWithoutReason),
            "whitespace is not a reason"
        );
        let t = plan_move(Stage::Data, Stage::Experiments, &ALL, admin(),
                          &full_evidence(), Some("protocol was wrong"), false).unwrap();
        assert_eq!(t.direction, Direction::Back);
        // Going back is not gated on the forward guards.
        assert!(t.checks.is_empty());
    }

    // ---- skipped stages ----

    #[test]
    fn a_project_that_skips_funding_moves_hypothesis_to_experiments() {
        let enabled = [Stage::Idea, Stage::Hypothesis, Stage::Experiments, Stage::Published];
        assert_eq!(next_enabled(Stage::Hypothesis, &enabled), Some(Stage::Experiments));
        let t = plan_move(Stage::Hypothesis, Stage::Experiments, &enabled, admin(),
                          &full_evidence(), None, false).unwrap();
        assert_eq!(t.direction, Direction::Forward,
                   "the next ENABLED stage is a forward move, not a skip");
    }

    #[test]
    fn jumping_past_an_enabled_stage_is_recorded_as_a_skip() {
        let t = plan_move(Stage::Idea, Stage::Funding, &ALL, admin(),
                          &full_evidence(), None, false).unwrap();
        assert_eq!(t.direction, Direction::Skip);
    }

    #[test]
    fn a_stage_the_project_does_not_use_is_refused() {
        let enabled = [Stage::Idea, Stage::Experiments, Stage::Published];
        assert_eq!(
            plan_move(Stage::Idea, Stage::Hypothesis, &enabled, admin(),
                      &full_evidence(), None, false),
            Err(Refusal::StageNotEnabled(Stage::Hypothesis))
        );
    }

    #[test]
    fn moving_to_the_same_stage_goes_nowhere() {
        assert_eq!(
            plan_move(Stage::Idea, Stage::Idea, &ALL, admin(),
                      &full_evidence(), None, false),
            Err(Refusal::SameStage)
        );
    }

    // ---- schema conformance ----
    //
    // The point of the model is that the JSON Schema is the contract. These
    // validate what THIS code emits against the vendored schema, so a drift
    // between the Rust types and the contract fails here rather than at a
    // peer's node.

    /// The vendored contract. Its ROOT is `$ref: #/$defs/record`, so the
    /// document validates a record directly — no wrapper needed.
    fn model_schema() -> serde_json::Value {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../schemas/olb-model.schema.json");
        serde_json::from_str(&std::fs::read_to_string(p).expect("schemas/olb-model.schema.json not found"))
            .expect("schema is not valid json")
    }

    /// Serves the two sibling schemas the model refers to by absolute URI, so
    /// building the validator never reaches the network.
    struct Vendored;

    impl jsonschema::Retrieve for Vendored {
        fn retrieve(
            &self,
            uri: &jsonschema::Uri<String>,
        ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
            let name = uri.as_str().rsplit('/').next().unwrap_or_default();
            let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../schemas/");
            match name {
                "collection-protocol.schema.json" | "submission.schema.json" => {
                    Ok(serde_json::from_str(&std::fs::read_to_string(format!("{dir}{name}"))?)?)
                }
                other => Err(format!("unexpected schema reference: {other}").into()),
            }
        }
    }

    fn validator() -> jsonschema::Validator {
        jsonschema::options()
            .with_retriever(Vendored)
            .build(&model_schema())
            .expect("schema did not compile")
    }

    fn validate_record(v: &serde_json::Value) {
        let errs: Vec<String> = validator()
            .iter_errors(v)
            .map(|e| format!("{}: {e}", e.instance_path()))
            .collect();
        assert!(errs.is_empty(), "record rejected by the schema:\n  {}", errs.join("\n  "));
    }

    fn envelope(kind: RecordType, fields: serde_json::Value) -> serde_json::Value {
        let e = Envelope {
            id: "olb:obj:qpx4g2mz7k3n8r5t".into(),
            kind,
            version: 1,
            project: "olb:proj:bkvglqcziuv4ig45".into(),
            authors: vec![Actor {
                key: "dd028f673cc71c456c1eb6b253bf5a3b894a6fe6172d7391ce49ba440c205b33".into(),
                kind: ActorKind::Member,
                name: Some("Imtiyaz".into()),
                operator: None,
            }],
            parents: vec![],
            created_at: "2026-10-28T12:40:00Z".into(),
            fields: fields.as_object().unwrap().clone(),
            sig: "a".repeat(128),
            co_sigs: vec![],
        };
        serde_json::to_value(e).unwrap()
    }

    #[test]
    fn our_stage_transition_validates_against_the_schema() {
        let t = plan_move(Stage::Idea, Stage::Hypothesis, &ALL, admin(),
                          &full_evidence(), None, false).unwrap();
        validate_record(&envelope(RecordType::StageTransition, serde_json::to_value(&t).unwrap()));
    }

    #[test]
    fn a_back_transition_validates_and_carries_its_reason() {
        let t = plan_move(Stage::Data, Stage::Experiments, &ALL, admin(),
                          &full_evidence(), Some("protocol was wrong"), false).unwrap();
        validate_record(&envelope(RecordType::StageTransition, serde_json::to_value(&t).unwrap()));
    }

    #[test]
    fn an_override_transition_validates() {
        let mut ev = full_evidence();
        ev.grant_funded = false;
        let t = plan_move(Stage::Funding, Stage::Experiments, &ALL, admin(), &ev,
                          Some("self-funded"), true).unwrap();
        validate_record(&envelope(RecordType::StageTransition, serde_json::to_value(&t).unwrap()));
    }

    #[test]
    fn the_start_transition_validates() {
        validate_record(&envelope(RecordType::StageTransition,
                                  serde_json::to_value(start(Stage::Idea)).unwrap()));
    }

    // ---- field types ----

    fn settings() -> Settings {
        Settings {
            anchoring: Anchoring::Batched,
            merge_approvals: Some(1),
            replica: Some(true),
            allow_visitors: None,
            auto_tags: Some(true),
            reviewers_can_verify: None,
        }
    }

    fn project_fields() -> ProjectFields {
        ProjectFields {
            title: "Bird Spotting Dhun Rajasthan".into(),
            summary: Some("Counting nesting pairs across three districts.".into()),
            description: Some(RichDoc::paragraph("Field survey over one season.")),
            field: Some("Ecology".into()),
            keywords: vec!["birds".into(), "rajasthan".into()],
            licence: Some(Licence::CcBy40),
            privacy: Privacy::Open,
            stages_enabled: Stage::ALL.to_vec(),
            settings: settings(),
            references: vec![Reference {
                kind: ReferenceKind::Doi,
                identifier: "10.1000/example".into(),
                title: Some("A prior survey".into()),
                authors: vec!["Mehta, A.".into()],
                year: Some(2024),
            }],
            start_date: Some("2026-11-01".into()),
            expected_duration: Some(Duration::SixToTwelveMonths),
            team_size: Some(TeamSize::TwoToFive),
            existing_grant: None,
        }
    }

    #[test]
    fn our_project_record_validates_against_the_schema() {
        let v = envelope(RecordType::Project, serde_json::to_value(project_fields()).unwrap());
        // A project record's id must be the PROJECT id, not an object id.
        let mut v = v;
        v["id"] = json!("olb:proj:bkvglqcziuv4ig45");
        validate_record(&v);
    }

    /// The schema's own negative test, from our side: a project record
    /// carrying an object id is refused.
    #[test]
    fn a_project_record_with_an_object_id_is_refused() {
        let v = envelope(RecordType::Project, serde_json::to_value(project_fields()).unwrap());
        assert!(validator().iter_errors(&v).next().is_some(),
                "olb:obj: id on a project record should not validate");
    }

    #[test]
    fn enum_spellings_match_the_schema() {
        assert_eq!(serde_json::to_value(Privacy::InviteOnly).unwrap(), json!("invite_only"));
        assert_eq!(serde_json::to_value(Licence::CcBy40).unwrap(), json!("CC-BY-4.0"));
        assert_eq!(serde_json::to_value(Anchoring::Milestones).unwrap(), json!("milestones"));
        assert_eq!(serde_json::to_value(Duration::SixToTwelveMonths).unwrap(), json!("6_12m"));
        assert_eq!(serde_json::to_value(TeamSize::OverFifteen).unwrap(), json!("gt_15"));
        assert_eq!(serde_json::to_value(RichFormat::OlbBlocks1).unwrap(), json!("olb-blocks/1"));
        assert_eq!(serde_json::to_value(MemberStatus::Removed).unwrap(), json!("removed"));
        assert_eq!(serde_json::to_value(InviteKind::ByCode).unwrap(), json!("by_code"));
        assert_eq!(serde_json::to_value(IdeaKind::Evidence).unwrap(), json!("evidence"));
        assert_eq!(serde_json::to_value(Role::Reviewer).unwrap(), json!("reviewer"));
    }

    #[test]
    fn our_idea_contribution_validates() {
        let f = IdeaContributionFields {
            kind: IdeaKind::Proposal,
            body: RichDoc::paragraph("Survey the eastern district first."),
            reply_to: None,
            references: vec![],
        };
        validate_record(&envelope(RecordType::IdeaContribution, serde_json::to_value(f).unwrap()));
    }

    #[test]
    fn our_membership_validates() {
        let f = MembershipFields {
            member: "b".repeat(64),
            name: "Arjun Mehta".into(),
            role: Role::Reviewer,
            status: MemberStatus::Active,
            invited_by: Some("dd028f673cc71c456c1eb6b253bf5a3b894a6fe6172d7391ce49ba440c205b33".into()),
            invite_kind: Some(InviteKind::ById),
            key_version: Some(1),
        };
        validate_record(&envelope(RecordType::Membership, serde_json::to_value(f).unwrap()));
    }

    // ---- stages_enabled invariants ----

    #[test]
    fn stages_enabled_must_end_at_published() {
        assert_eq!(
            validate_stages(&[Stage::Idea, Stage::Data]),
            Err(StagesInvalid::DoesNotEndAtPublished)
        );
        assert!(validate_stages(&[Stage::Idea, Stage::Published]).is_ok());
        assert!(validate_stages(&Stage::ALL).is_ok());
    }

    #[test]
    fn stages_enabled_must_be_in_lifecycle_order() {
        assert_eq!(
            validate_stages(&[Stage::Hypothesis, Stage::Idea, Stage::Published]),
            Err(StagesInvalid::OutOfOrder)
        );
    }

    #[test]
    fn stages_enabled_rejects_duplicates_and_emptiness() {
        assert_eq!(
            validate_stages(&[Stage::Idea, Stage::Idea, Stage::Published]),
            Err(StagesInvalid::Duplicate(Stage::Idea))
        );
        assert_eq!(validate_stages(&[]), Err(StagesInvalid::Empty));
    }

    /// A project may skip the middle entirely, as long as it ends published.
    #[test]
    fn a_minimal_project_is_idea_then_published() {
        let stages = [Stage::Idea, Stage::Published];
        assert!(validate_stages(&stages).is_ok());
        assert_eq!(next_enabled(Stage::Idea, &stages), Some(Stage::Published));
    }

    /// An inline carries no type tag. Adding `"type": "text"` is the obvious
    /// thing to reach for and the schema rejects it outright
    /// (`additionalProperties: false`), which cost a test cycle to find.
    #[test]
    fn an_inline_has_text_and_no_type_tag() {
        let v = serde_json::to_value(Inline::text("hello")).unwrap();
        assert_eq!(v, json!({"text": "hello"}));
        assert!(v.get("type").is_none());
        assert!(v.get("marks").is_none(), "empty marks are omitted");

        let marked = Inline { text: "x".into(), marks: vec![Mark::Bold, Mark::Code],
                              href: None, mention: None };
        assert_eq!(serde_json::to_value(marked).unwrap(),
                   json!({"text": "x", "marks": ["bold", "code"]}));
    }

    #[test]
    fn a_rich_doc_paragraph_round_trips() {
        let d = RichDoc::paragraph("hello");
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["format"], json!("olb-blocks/1"));
        assert_eq!(v["blocks"][0]["type"], json!("paragraph"));
        assert!(v.get("crdt_state").is_none(), "omitted when absent");
        let back: RichDoc = serde_json::from_value(v).unwrap();
        assert_eq!(back, d);
    }

    // ---- sealing ----

    fn key() -> ed25519_dalek::SigningKey {
        // Fixed bytes so the test is deterministic; never a real key.
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
    }

    fn author_of(k: &ed25519_dalek::SigningKey) -> Actor {
        Actor {
            key: hex::encode(k.verifying_key().to_bytes()),
            kind: ActorKind::Member,
            name: Some("Imtiyaz".into()),
            operator: None,
        }
    }

    #[test]
    fn a_sealed_record_verifies_and_validates() {
        let k = key();
        let env = seal_record(
            &k,
            RecordType::IdeaContribution,
            "olb:proj:bkvglqcziuv4ig45",
            1,
            None,
            vec![author_of(&k)],
            vec![],
            "2026-10-28T13:20:00Z".into(),
            serde_json::to_value(IdeaContributionFields {
                kind: IdeaKind::Proposal,
                body: RichDoc::paragraph("Start with the eastern district."),
                reply_to: None,
                references: vec![],
            })
            .unwrap(),
        )
        .unwrap();

        assert!(env.id.starts_with("olb:obj:"), "got {}", env.id);
        assert!(verify_record(&env, &k.verifying_key()));
        validate_record(&serde_json::to_value(&env).unwrap());
    }

    /// The id and the signature cover the same bytes, so editing a record
    /// breaks its signature rather than quietly changing its meaning.
    #[test]
    fn tampering_with_a_sealed_record_breaks_its_signature() {
        let k = key();
        let mut env = seal_record(
            &k, RecordType::StageTransition, "olb:proj:bkvglqcziuv4ig45", 1, None,
            vec![author_of(&k)], vec![], "2026-10-28T13:20:00Z".into(),
            serde_json::to_value(start(Stage::Idea)).unwrap(),
        ).unwrap();
        assert!(verify_record(&env, &k.verifying_key()));

        env.fields.insert("to".into(), json!("published"));
        assert!(!verify_record(&env, &k.verifying_key()),
                "a changed field must invalidate the signature");
    }

    #[test]
    fn another_key_does_not_verify() {
        let k = key();
        let env = seal_record(
            &k, RecordType::StageTransition, "olb:proj:bkvglqcziuv4ig45", 1, None,
            vec![author_of(&k)], vec![], "2026-10-28T13:20:00Z".into(),
            serde_json::to_value(start(Stage::Idea)).unwrap(),
        ).unwrap();
        let other = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        assert!(!verify_record(&env, &other.verifying_key()));
    }

    /// Identical content from two members gives the same id, so creation can
    /// never conflict — the v1 property, kept.
    #[test]
    fn the_id_is_the_hash_of_the_content() {
        let k = key();
        let make = || seal_record(
            &k, RecordType::IdeaContribution, "olb:proj:x", 1, None,
            vec![author_of(&k)], vec![], "2026-10-28T13:20:00Z".into(),
            serde_json::to_value(IdeaContributionFields {
                kind: IdeaKind::Comment,
                body: RichDoc::paragraph("same"),
                reply_to: None, references: vec![],
            }).unwrap(),
        ).unwrap();
        assert_eq!(make().id, make().id);

        let different = seal_record(
            &k, RecordType::IdeaContribution, "olb:proj:x", 1, None,
            vec![author_of(&k)], vec![], "2026-10-28T13:20:00Z".into(),
            serde_json::to_value(IdeaContributionFields {
                kind: IdeaKind::Comment,
                body: RichDoc::paragraph("different"),
                reply_to: None, references: vec![],
            }).unwrap(),
        ).unwrap();
        assert_ne!(make().id, different.id);
    }

    #[test]
    fn a_v2_preimage_cannot_collide_with_a_v1_one() {
        assert_ne!(RECORD_V2, crate::hashing::RECORD);
        let body = b"same bytes";
        assert_ne!(
            crate::hashing::tagged(RECORD_V2, body),
            crate::hashing::tagged(crate::hashing::RECORD, body)
        );
    }

    /// An object id is the hash of version 1. Asking seal_record to mint one
    /// for a later version is a bug, not a fallback.
    #[test]
    fn a_later_version_must_carry_its_object_id() {
        let k = key();
        let err = seal_record(
            &k, RecordType::IdeaContribution, "olb:proj:x", 2, None,
            vec![author_of(&k)], vec![], "2026-10-28T13:20:00Z".into(),
            json!({"kind": "comment", "body": {"format":"olb-blocks/1","blocks":[]}}),
        );
        assert!(err.is_err(), "version 2 with no id should be refused");

        // With the id carried over it is fine.
        let ok = seal_record(
            &k, RecordType::IdeaContribution, "olb:proj:x", 2,
            Some("olb:obj:abc".into()),
            vec![author_of(&k)], vec![], "2026-10-28T13:20:00Z".into(),
            json!({"kind": "comment", "body": {"format":"olb-blocks/1","blocks":[]}}),
        ).unwrap();
        assert_eq!(ok.id, "olb:obj:abc");
        assert_eq!(ok.version, 2);
    }

    // ---- history events (§6) ----

    fn event() -> HistoryEvent {
        HistoryEvent {
            commit: "a41c9e2".into(),
            at: "2026-10-28T13:10:00Z".into(),
            actor: Actor {
                key: "dd028f673cc71c456c1eb6b253bf5a3b894a6fe6172d7391ce49ba440c205b33".into(),
                kind: ActorKind::Member,
                name: Some("Imtiyaz".into()),
                operator: None,
            },
            action: Action::StageMoved,
            stage: Stage::Hypothesis,
            target: Target {
                kind: "project".into(),
                id: "olb:proj:bkvglqcziuv4ig45".into(),
                title: Some("Bird Spotting".into()),
                version: None,
            },
            protocol: None,
            detail: Some("Idea complete".into()),
            anchor: Anchor::pending(),
        }
    }

    fn validate_history(v: &serde_json::Value) {
        let schema = model_schema();
        let id = schema["$id"].as_str().unwrap().to_string();
        let wrapper = json!({"$ref": format!("{id}#/$defs/historyEvent")});
        // The event schema is reached by pointer, so the document itself has
        // to be retrievable by its own $id.
        struct Me(serde_json::Value, String);
        impl jsonschema::Retrieve for Me {
            fn retrieve(
                &self,
                uri: &jsonschema::Uri<String>,
            ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
                if uri.as_str() == self.1 {
                    return Ok(self.0.clone());
                }
                let name = uri.as_str().rsplit('/').next().unwrap_or_default();
                let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../schemas/");
                Ok(serde_json::from_str(&std::fs::read_to_string(format!("{dir}{name}"))?)?)
            }
        }
        let v8 = jsonschema::options()
            .with_retriever(Me(schema.clone(), id))
            .build(&wrapper)
            .expect("history schema did not compile");
        let errs: Vec<String> = v8.iter_errors(v).map(|e| format!("{}: {e}", e.instance_path())).collect();
        assert!(errs.is_empty(), "history event rejected:\n  {}", errs.join("\n  "));
    }

    #[test]
    fn our_history_event_validates_against_the_schema() {
        validate_history(&serde_json::to_value(event()).unwrap());
    }

    #[test]
    fn every_action_name_is_snake_case_on_the_wire() {
        assert_eq!(serde_json::to_value(Action::StageMoved).unwrap(), json!("stage_moved"));
        assert_eq!(serde_json::to_value(Action::PaperPublished).unwrap(), json!("paper_published"));
        assert_eq!(serde_json::to_value(Action::CreditResponded).unwrap(), json!("credit_responded"));
        // All 32 must be accepted by the schema's enum.
        let allowed = model_schema()["$defs"]["historyEvent"]["properties"]["action"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(allowed.len(), 32, "the schema lists 32 actions");
        for a in [Action::ProjectCreated, Action::SettingsChanged, Action::MemberInvited,
                  Action::MemberJoined, Action::MemberRoleChanged, Action::MemberRemoved,
                  Action::JoinRequested, Action::JoinDecided, Action::StageMoved,
                  Action::IdeaContributed, Action::HypothesisEdited, Action::HypothesisReviewed,
                  Action::HypothesisCommitted, Action::HypothesisRevealed, Action::GrantDrafted,
                  Action::GrantOpened, Action::PledgeMade, Action::MilestoneDecided,
                  Action::MilestoneReleased, Action::GrantClosed, Action::ProtocolDrafted,
                  Action::ProtocolPublished, Action::ProtocolClosed, Action::SubmissionAdded,
                  Action::SubmissionCorrected, Action::SubmissionReviewed, Action::SnapshotFrozen,
                  Action::AnalysisEdited, Action::ManuscriptEdited, Action::ManuscriptReady,
                  Action::PaperPublished, Action::CreditResponded] {
            let name = serde_json::to_value(a).unwrap().as_str().unwrap().to_string();
            assert!(allowed.contains(&name), "{name} is not in the schema's action enum");
        }
    }

    #[test]
    fn an_event_starts_unanchored() {
        let v = serde_json::to_value(Anchor::pending()).unwrap();
        assert_eq!(v, json!({"state": "pending"}), "batch omitted until anchored");
    }

    // ---- strict parsing ----
    //
    // The schema says additionalProperties: false. serde should agree at
    // PARSE time, so a record from a peer carrying an unexpected field is
    // refused before anything acts on it, rather than silently ignored.

    #[test]
    fn an_unknown_field_is_refused_when_parsing() {
        let good = json!({"key": "a".repeat(64), "kind": "member"});
        assert!(serde_json::from_value::<Actor>(good).is_ok());

        let sneaky = json!({"key": "a".repeat(64), "kind": "member", "role": "admin"});
        let err = serde_json::from_value::<Actor>(sneaky).unwrap_err().to_string();
        assert!(err.contains("role"), "expected the unknown field named, got: {err}");
    }

    #[test]
    fn an_unknown_field_in_a_payload_is_refused_too() {
        let v = json!({
            "title": "x", "privacy": "open",
            "stages_enabled": ["idea", "published"],
            "settings": {"anchoring": "batched"},
            "stage": 2
        });
        let err = serde_json::from_value::<ProjectFields>(v).unwrap_err().to_string();
        assert!(err.contains("stage"),
                "a v1 `stage` field must not be quietly accepted: {err}");
    }

    /// An agent author without its operator must be rejected — the schema
    /// enforces attribution, and we rely on that rather than re-checking it.
    #[test]
    fn the_schema_rejects_an_agent_without_an_operator() {
        let mut v = envelope(RecordType::StageTransition,
                             serde_json::to_value(start(Stage::Idea)).unwrap());
        v["authors"][0]["kind"] = json!("agent");
        v["authors"][0].as_object_mut().unwrap().remove("operator");
        assert!(validator().iter_errors(&v).next().is_some(),
                "an agent with no operator should not validate");
    }
}
