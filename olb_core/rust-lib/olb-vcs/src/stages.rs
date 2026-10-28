//! Where a project has reached in its six stages (docs/03, docs/08 D18).
//!
//! Two decisions shape everything here, both taken 28 Oct 2026:
//!
//! 1. **A stage belongs to the project, not to each idea.** All the work
//!    inside a project is at the project's stage. An idea records the stage
//!    it entered at, but the project is what moves.
//! 2. **A stage is only ever finished by doing its work.** There is no
//!    control that sets a stage. So every question below is about what the
//!    project *contains*, never about what someone pressed — which is why
//!    this module is pure functions over the project document and can be
//!    tested without a store, a key or a running module.
//!
//! A consequence worth stating plainly: a stage whose screen has not been
//! built yet has nothing that can produce its work, so it reports incomplete
//! and holds the project where it is. That is the design working, not a
//! stall.

use serde_json::Value;

/// The six stages, in order. A project may use a subset.
pub const STAGES: [&str; 6] = [
    "Idea",
    "Hypothesis",
    "Funding",
    "Experiments",
    "Data",
    "Publish",
];

/// Normalise the aliases the UI offers ("Experiment", "Data/Results").
fn canon(stage: &str) -> String {
    let n = stage.trim().to_ascii_lowercase();
    match n.as_str() {
        "experiment" => "experiments".into(),
        "data/results" | "results" => "data".into(),
        _ => n,
    }
}

/// 1-based position of a stage in the canonical six, if it is one of them.
pub fn stage_index(name: &str) -> Option<usize> {
    let n = canon(name);
    STAGES
        .iter()
        .position(|s| s.to_ascii_lowercase() == n)
        .map(|i| i + 1)
}

/// What the named stage is still waiting for, in the words the UI shows.
///
/// A stage that cannot be finished yet has to say why, or work-driven
/// advancement just looks like nothing happening.
pub fn stage_needs(stage: &str) -> &'static str {
    match canon(stage).as_str() {
        "idea" => "Add an idea to this project",
        "hypothesis" => "Pre-register a hypothesis",
        "funding" => "Record a funding decision",
        "experiments" => "Record a protocol run",
        "data" => "Attach a dataset",
        "publish" => "Publish an idea",
        _ => "",
    }
}

/// Whether the work that defines `stage` has been done for this project.
pub fn stage_complete(project: &Value, stage: &str) -> bool {
    let non_empty = |key: &str| {
        project
            .get(key)
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty())
    };
    match canon(stage).as_str() {
        "idea" => non_empty("ideas"),
        "hypothesis" => non_empty("hypotheses"),
        "funding" => non_empty("funding"),
        "experiments" => non_empty("runs"),
        "data" => non_empty("datasets"),
        // Publishing is a property of the work, not a list of its own.
        "publish" => project
            .get("ideas")
            .and_then(|v| v.as_array())
            .is_some_and(|a| {
                a.iter()
                    .any(|i| i.get("published").and_then(|p| p.as_bool()) == Some(true))
            }),
        _ => false,
    }
}

/// The stages this project uses, falling back to all six.
pub fn stages_of(project: &Value) -> Vec<String> {
    project
        .get("stages")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| STAGES.iter().map(|s| s.to_string()).collect())
}

/// The stage this project has reached, as a 1-based position in its OWN
/// stage list — a project that skipped Funding has no Funding to be at.
///
/// Walks forward while each stage's work is done, so finishing several
/// stages at once moves past all of them rather than one step per action.
/// The last stage is terminal: completing Publish leaves the project at
/// Publish rather than off the end of its own list.
pub fn reached_stage(project: &Value) -> usize {
    let stages = stages_of(project);
    let mut at = 1usize;
    while at < stages.len() && stage_complete(project, &stages[at - 1]) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project(stages: &[&str], extra: Value) -> Value {
        let mut p = json!({"id": "olb:proj:x", "stages": stages, "ideas": []});
        if let Some(obj) = extra.as_object() {
            for (k, v) in obj {
                p[k] = v.clone();
            }
        }
        p
    }

    #[test]
    fn a_new_project_is_at_its_first_stage() {
        let p = project(&STAGES, json!({}));
        assert_eq!(reached_stage(&p), 1);
        assert!(!stage_complete(&p, "Idea"));
    }

    #[test]
    fn adding_an_idea_finishes_the_idea_stage() {
        let p = project(&STAGES, json!({"ideas": [{"title": "n"}]}));
        assert!(stage_complete(&p, "Idea"));
        assert_eq!(reached_stage(&p), 2, "moves to Hypothesis and stops there");
    }

    /// The point of work-driven advancement: an idea does not carry the
    /// project through stages whose work does not exist.
    #[test]
    fn it_stops_at_the_first_unfinished_stage() {
        let p = project(&STAGES, json!({"ideas": [{"title": "n"}]}));
        assert_eq!(reached_stage(&p), 2);
        assert_eq!(stage_needs("Hypothesis"), "Pre-register a hypothesis");
    }

    #[test]
    fn finishing_several_stages_moves_past_all_of_them() {
        let p = project(
            &STAGES,
            json!({
                "ideas": [{"title": "n"}],
                "hypotheses": [{"id": "h1"}],
                "funding": [{"id": "f1"}],
            }),
        );
        assert_eq!(reached_stage(&p), 4, "Idea, Hypothesis, Funding all done");
    }

    /// A project's position is in ITS stages. Skipping Funding must not
    /// leave the project pointing at a stage it does not use.
    #[test]
    fn position_is_relative_to_the_projects_own_stages() {
        let p = project(
            &["Idea", "Experiments", "Publish"],
            json!({"ideas": [{"title": "n"}]}),
        );
        let at = reached_stage(&p);
        assert_eq!(at, 2);
        assert_eq!(stages_of(&p)[at - 1], "Experiments", "not Hypothesis");
    }

    #[test]
    fn the_last_stage_is_terminal() {
        let p = project(
            &["Idea", "Publish"],
            json!({"ideas": [{"title": "n", "published": true}]}),
        );
        assert!(stage_complete(&p, "Publish"));
        assert_eq!(reached_stage(&p), 2, "stays at Publish, not 3");
    }

    #[test]
    fn publish_needs_a_published_idea_not_merely_an_idea() {
        let unpublished = project(&["Idea", "Publish"], json!({"ideas": [{"title": "n"}]}));
        assert!(!stage_complete(&unpublished, "Publish"));
    }

    #[test]
    fn aliases_the_ui_offers_are_accepted() {
        assert_eq!(stage_index("Experiment"), stage_index("Experiments"));
        assert_eq!(stage_index("Data/Results"), stage_index("Data"));
        let p = project(&STAGES, json!({"runs": [{"id": "r1"}]}));
        assert!(stage_complete(&p, "Experiment"));
    }

    #[test]
    fn an_empty_stage_list_falls_back_to_all_six() {
        let p = json!({"id": "x", "stages": [], "ideas": []});
        assert_eq!(stages_of(&p).len(), 6);
        assert_eq!(reached_stage(&p), 1);
    }

    #[test]
    fn an_unknown_stage_is_never_complete() {
        let p = project(&STAGES, json!({"ideas": [{"title": "n"}]}));
        assert!(!stage_complete(&p, "Peer review"));
        assert_eq!(stage_needs("Peer review"), "");
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use serde_json::json;

    /// Projects created before `stage` existed have no such field, and are
    /// never migrated. Position is derived from the work, so an old project
    /// must still report a sensible stage rather than 0 or a panic.
    #[test]
    fn a_project_with_no_stage_field_still_reports_one() {
        let legacy = json!({
            "id": "proj-18e2012cb69b69d0",
            "title": "Test Project Data Collection",
            "stages": ["Idea", "Hypothesis", "Funding", "Experiments", "Data", "Publish"],
            "ideas": [],
        });
        assert!(legacy.get("stage").is_none(), "precondition: no stage field");
        assert_eq!(reached_stage(&legacy), 1);
    }

    /// Nor does a project that predates `stages` — the fallback is all six.
    #[test]
    fn a_project_with_neither_stage_nor_stages_is_at_idea() {
        let ancient = json!({"id": "p", "title": "old", "ideas": []});
        assert_eq!(reached_stage(&ancient), 1);
        assert_eq!(stages_of(&ancient)[0], "Idea");
    }
}
