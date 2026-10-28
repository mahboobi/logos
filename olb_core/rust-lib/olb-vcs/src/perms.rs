//! Who may do what, and when (docs/13 §5 and the edit windows in §4).
//!
//! This lives in a crate `olb_core` links, not in the UI, because §5 asks for
//! it to be enforced in the core: a hidden button is not a permission, and a
//! hand-built IPC call must hit the same wall a screen does.
//!
//! Pure functions over role, privacy and context, so every row of the matrix
//! can be tested without a store, a key or a project.

use crate::model::{Role, Stage};

/// Project privacy, chosen once at creation and never changed (§2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privacy {
    Open,
    InviteOnly,
    Private,
}

/// Who is asking. An external user is anyone with no membership record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Member(Role),
    External,
}

impl Who {
    pub const ADMIN: Who = Who::Member(Role::Admin);
    pub const MEMBER: Who = Who::Member(Role::Member);
    pub const REVIEWER: Who = Who::Member(Role::Reviewer);

    fn role(self) -> Option<Role> {
        match self {
            Who::Member(r) => Some(r),
            Who::External => None,
        }
    }
}

/// What kind of idea contribution. Reviewers and visitors may only comment,
/// which is a limit on the KIND rather than on the action (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeaKind {
    Proposal,
    Comment,
    Question,
    Evidence,
}

/// The actions the matrix covers. Context that changes the answer is carried
/// in the variant rather than passed alongside, so a caller cannot forget it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Read private and restricted content.
    ReadRestricted,
    AddIdeaContribution(IdeaKind),
    /// Write or edit a hypothesis.
    WriteHypothesis,
    ReviewHypothesis,
    /// Commit it on-chain. A member may only propose.
    CommitHypothesis,
    /// Open a grant request for pledges.
    OpenGrant,
    /// Draft one without opening it.
    DraftGrant,
    Pledge,
    /// Approve a milestone. Allowed only to someone NAMED on that grant,
    /// whatever their role — and §5's closing rule forbids a named reviewer
    /// from also being an admin or member.
    ApproveMilestone { named_on_grant: bool },
    CreateProtocol,
    PublishProtocol,
    SubmitData { protocol_allows_visitors: bool },
    /// Whether reviewers may verify is a project setting.
    VerifySubmission { reviewers_may_verify: bool },
    /// Write analysis or manuscript, or freeze a snapshot.
    WriteAnalysis,
    CommentOnAnalysis,
    MoveStage,
    PublishPaper,
    /// Invite, remove, or change a role.
    ManageMembers,
    CommentOnPublishedPaper,
}

/// Whether `who` may perform `action` in a project of this `privacy`.
///
/// §5's external column describes an **Open** project; for invite-only and
/// private projects the narrower rules in §2 apply, which is why privacy is
/// a parameter rather than assumed.
pub fn allows(who: Who, privacy: Privacy, action: Action) -> bool {
    use Action::*;
    use Role::*;

    // An external user in a project that does not publish its content can do
    // almost nothing until publication (§2). Handled first so no row below
    // has to repeat it.
    if who == Who::External && privacy != Privacy::Open {
        return matches!(action, Pledge | CommentOnPublishedPaper);
    }

    match action {
        ReadRestricted => who.role().is_some(),

        // Reviewers and visitors may comment, nothing more.
        AddIdeaContribution(kind) => match who {
            Who::Member(Admin) | Who::Member(Member) => true,
            Who::Member(Reviewer) | Who::External => kind == IdeaKind::Comment,
        },

        WriteHypothesis => matches!(who, Who::Member(Admin) | Who::Member(Member)),
        ReviewHypothesis => who.role().is_some(),
        CommitHypothesis => who == Who::Member(Admin),

        OpenGrant => who == Who::Member(Admin),
        DraftGrant => matches!(who, Who::Member(Admin) | Who::Member(Member)),

        // Anyone allowed to see the grant may back it, including visitors.
        Pledge => true,

        // Role is not what qualifies someone here; being named is.
        ApproveMilestone { named_on_grant } => named_on_grant && who.role().is_some(),

        CreateProtocol | PublishProtocol => {
            matches!(who, Who::Member(Admin) | Who::Member(Member))
        }

        SubmitData {
            protocol_allows_visitors,
        } => match who {
            Who::Member(Admin) | Who::Member(Member) => true,
            Who::Member(Reviewer) => false,
            Who::External => protocol_allows_visitors,
        },

        VerifySubmission {
            reviewers_may_verify,
        } => match who {
            Who::Member(Admin) | Who::Member(Member) => true,
            Who::Member(Reviewer) => reviewers_may_verify,
            Who::External => false,
        },

        WriteAnalysis => matches!(who, Who::Member(Admin) | Who::Member(Member)),
        CommentOnAnalysis => who.role().is_some(),

        MoveStage | PublishPaper | ManageMembers => who == Who::Member(Admin),

        CommentOnPublishedPaper => true,
    }
}

/// §5: "a named reviewer cannot also be a member with role Admin or Member."
///
/// Researchers must not approve funding for their own project, so this is
/// checked when naming reviewers on a grant — not when the decision arrives,
/// by which point the conflict already exists.
pub fn reviewer_conflicts(role_in_project: Option<Role>) -> bool {
    matches!(role_in_project, Some(Role::Admin) | Some(Role::Member))
}

/// Items with a stage-bound edit window (§4, "What can be edited in which
/// stage"). Separate from [`allows`]: having the right role is not the same
/// as the project being at a stage where the item may still change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    IdeaContribution,
    /// Comments on a contribution stay open after the project moves on.
    IdeaComment,
    /// Editable until committed, then read-only until revealed.
    Hypothesis { committed: bool },
    /// Editable while draft; afterwards only status and milestone decisions.
    GrantRequest { opened: bool },
    ProtocolDraft,
    ProtocolPublishOrClose,
    Submission { protocol_active: bool, within_edit_window: bool },
    Analysis,
    Manuscript,
    /// Immutable once frozen.
    DatasetSnapshot { frozen: bool },
    Publication { exists: bool },
}

/// Whether `item` may still be changed with the project at `stage`.
pub fn editable_at(item: Item, stage: Stage) -> bool {
    use Item::*;
    match item {
        IdeaContribution => stage == Stage::Idea,
        // Deliberately any stage: discussion does not close when work moves on.
        IdeaComment => true,
        Hypothesis { committed } => !committed && stage == Stage::Hypothesis,
        GrantRequest { opened } => !opened && stage == Stage::Funding,
        ProtocolDraft => true,
        ProtocolPublishOrClose => stage >= Stage::Experiments,
        Submission {
            protocol_active,
            within_edit_window,
        } => protocol_active && within_edit_window && stage >= Stage::Experiments,
        Analysis | Manuscript => stage == Stage::Data,
        DatasetSnapshot { frozen } => !frozen && stage == Stage::Data,
        // Created once at Published; a revision is a new version.
        Publication { exists } => !exists && stage == Stage::Published,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;

    const OPEN: Privacy = Privacy::Open;

    // ---- the matrix, row by row ----

    #[test]
    fn only_members_read_restricted_content() {
        for w in [Who::ADMIN, Who::MEMBER, Who::REVIEWER] {
            assert!(allows(w, OPEN, ReadRestricted), "{w:?}");
        }
        assert!(!allows(Who::External, OPEN, ReadRestricted));
    }

    #[test]
    fn reviewers_and_visitors_may_only_comment_on_ideas() {
        for kind in [IdeaKind::Proposal, IdeaKind::Question, IdeaKind::Evidence] {
            assert!(allows(Who::ADMIN, OPEN, AddIdeaContribution(kind)));
            assert!(allows(Who::MEMBER, OPEN, AddIdeaContribution(kind)));
            assert!(!allows(Who::REVIEWER, OPEN, AddIdeaContribution(kind)), "{kind:?}");
            assert!(!allows(Who::External, OPEN, AddIdeaContribution(kind)), "{kind:?}");
        }
        assert!(allows(Who::REVIEWER, OPEN, AddIdeaContribution(IdeaKind::Comment)));
        assert!(allows(Who::External, OPEN, AddIdeaContribution(IdeaKind::Comment)));
    }

    #[test]
    fn a_member_proposes_a_hypothesis_but_only_an_admin_commits_it() {
        assert!(allows(Who::MEMBER, OPEN, WriteHypothesis));
        assert!(!allows(Who::MEMBER, OPEN, CommitHypothesis));
        assert!(allows(Who::ADMIN, OPEN, CommitHypothesis));
        // A reviewer reviews, and does not write.
        assert!(!allows(Who::REVIEWER, OPEN, WriteHypothesis));
        assert!(allows(Who::REVIEWER, OPEN, ReviewHypothesis));
    }

    #[test]
    fn a_member_drafts_a_grant_but_only_an_admin_opens_it() {
        assert!(allows(Who::MEMBER, OPEN, DraftGrant));
        assert!(!allows(Who::MEMBER, OPEN, OpenGrant));
        assert!(allows(Who::ADMIN, OPEN, OpenGrant));
        assert!(!allows(Who::REVIEWER, OPEN, DraftGrant));
    }

    #[test]
    fn anyone_may_pledge() {
        for w in [Who::ADMIN, Who::MEMBER, Who::REVIEWER, Who::External] {
            assert!(allows(w, OPEN, Pledge), "{w:?}");
        }
    }

    /// The whole point of the row: being named is what qualifies, not a role.
    #[test]
    fn milestones_are_approved_only_by_someone_named_on_the_grant() {
        for w in [Who::ADMIN, Who::MEMBER, Who::REVIEWER] {
            assert!(!allows(w, OPEN, ApproveMilestone { named_on_grant: false }), "{w:?}");
            assert!(allows(w, OPEN, ApproveMilestone { named_on_grant: true }), "{w:?}");
        }
        // An outsider cannot, even if somehow named.
        assert!(!allows(Who::External, OPEN, ApproveMilestone { named_on_grant: true }));
    }

    /// "Researchers never approve funding for their own project."
    #[test]
    fn a_named_reviewer_may_not_also_be_an_admin_or_member() {
        assert!(reviewer_conflicts(Some(Role::Admin)));
        assert!(reviewer_conflicts(Some(Role::Member)));
        assert!(!reviewer_conflicts(Some(Role::Reviewer)));
        assert!(!reviewer_conflicts(None), "an outside reviewer is the normal case");
    }

    #[test]
    fn visitors_submit_data_only_when_the_protocol_allows_it() {
        assert!(!allows(Who::External, OPEN, SubmitData { protocol_allows_visitors: false }));
        assert!(allows(Who::External, OPEN, SubmitData { protocol_allows_visitors: true }));
        // A reviewer never submits research content.
        assert!(!allows(Who::REVIEWER, OPEN, SubmitData { protocol_allows_visitors: true }));
    }

    #[test]
    fn reviewers_verify_submissions_only_if_the_project_says_so() {
        assert!(!allows(Who::REVIEWER, OPEN, VerifySubmission { reviewers_may_verify: false }));
        assert!(allows(Who::REVIEWER, OPEN, VerifySubmission { reviewers_may_verify: true }));
        // Members always may; the setting does not restrict them.
        assert!(allows(Who::MEMBER, OPEN, VerifySubmission { reviewers_may_verify: false }));
    }

    #[test]
    fn a_reviewer_comments_on_analysis_but_does_not_write_it() {
        assert!(!allows(Who::REVIEWER, OPEN, WriteAnalysis));
        assert!(allows(Who::REVIEWER, OPEN, CommentOnAnalysis));
        assert!(allows(Who::MEMBER, OPEN, WriteAnalysis));
    }

    #[test]
    fn stages_publishing_and_membership_are_admin_only() {
        for a in [MoveStage, PublishPaper, ManageMembers] {
            assert!(allows(Who::ADMIN, OPEN, a), "{a:?}");
            for w in [Who::MEMBER, Who::REVIEWER, Who::External] {
                assert!(!allows(w, OPEN, a), "{w:?} must not {a:?}");
            }
        }
    }

    #[test]
    fn everyone_may_comment_on_the_published_paper() {
        for w in [Who::ADMIN, Who::MEMBER, Who::REVIEWER, Who::External] {
            for p in [Privacy::Open, Privacy::InviteOnly, Privacy::Private] {
                assert!(allows(w, p, CommentOnPublishedPaper), "{w:?} {p:?}");
            }
        }
    }

    // ---- privacy ----

    /// §5's external column is for OPEN projects. Elsewhere a visitor can
    /// only pledge and comment on the paper.
    #[test]
    fn a_visitor_to_a_closed_project_can_do_almost_nothing() {
        for p in [Privacy::InviteOnly, Privacy::Private] {
            assert!(!allows(Who::External, p, AddIdeaContribution(IdeaKind::Comment)), "{p:?}");
            assert!(!allows(Who::External, p, ReadRestricted), "{p:?}");
            assert!(!allows(Who::External, p, SubmitData { protocol_allows_visitors: true }), "{p:?}");
            assert!(allows(Who::External, p, Pledge), "{p:?} pledging stays open");
            assert!(allows(Who::External, p, CommentOnPublishedPaper), "{p:?}");
        }
    }

    #[test]
    fn privacy_does_not_change_what_members_may_do() {
        for p in [Privacy::Open, Privacy::InviteOnly, Privacy::Private] {
            assert!(allows(Who::MEMBER, p, WriteAnalysis), "{p:?}");
            assert!(allows(Who::ADMIN, p, MoveStage), "{p:?}");
        }
    }

    // ---- edit windows (§4) ----

    #[test]
    fn an_idea_contribution_is_editable_only_at_idea_but_comments_never_close() {
        assert!(editable_at(Item::IdeaContribution, Stage::Idea));
        assert!(!editable_at(Item::IdeaContribution, Stage::Data));
        for s in Stage::ALL {
            assert!(editable_at(Item::IdeaComment, s), "comments stay open at {s:?}");
        }
    }

    #[test]
    fn a_committed_hypothesis_is_read_only() {
        assert!(editable_at(Item::Hypothesis { committed: false }, Stage::Hypothesis));
        assert!(!editable_at(Item::Hypothesis { committed: true }, Stage::Hypothesis));
        // And not editable from another stage even before committing.
        assert!(!editable_at(Item::Hypothesis { committed: false }, Stage::Funding));
    }

    #[test]
    fn an_opened_grant_request_stops_being_editable() {
        assert!(editable_at(Item::GrantRequest { opened: false }, Stage::Funding));
        assert!(!editable_at(Item::GrantRequest { opened: true }, Stage::Funding));
    }

    #[test]
    fn protocols_draft_any_time_but_publish_only_from_experiments() {
        for s in Stage::ALL {
            assert!(editable_at(Item::ProtocolDraft, s), "draft at {s:?}");
        }
        assert!(!editable_at(Item::ProtocolPublishOrClose, Stage::Hypothesis));
        assert!(editable_at(Item::ProtocolPublishOrClose, Stage::Experiments));
        assert!(editable_at(Item::ProtocolPublishOrClose, Stage::Data),
                "'Experiments or later' includes Data");
    }

    #[test]
    fn a_submission_needs_an_active_protocol_and_an_open_window() {
        let ok = Item::Submission { protocol_active: true, within_edit_window: true };
        assert!(editable_at(ok, Stage::Experiments));
        assert!(editable_at(ok, Stage::Data), "collection continues into Data");
        assert!(!editable_at(ok, Stage::Hypothesis));
        assert!(!editable_at(
            Item::Submission { protocol_active: false, within_edit_window: true },
            Stage::Experiments
        ));
        assert!(!editable_at(
            Item::Submission { protocol_active: true, within_edit_window: false },
            Stage::Experiments
        ));
    }

    #[test]
    fn a_frozen_snapshot_is_immutable() {
        assert!(editable_at(Item::DatasetSnapshot { frozen: false }, Stage::Data));
        assert!(!editable_at(Item::DatasetSnapshot { frozen: true }, Stage::Data));
    }

    #[test]
    fn a_publication_is_created_once() {
        assert!(editable_at(Item::Publication { exists: false }, Stage::Published));
        assert!(!editable_at(Item::Publication { exists: true }, Stage::Published));
        assert!(!editable_at(Item::Publication { exists: false }, Stage::Data));
    }
}
