// Fixture data for the screens docs/03 calls S3, S4 and S5.
//
// CLAUDE.md asks every screen to be built against fixtures/sample-project.json
// first and wired to live data second. S1 and S2 get theirs from olb_core
// through the olb_ui backend, but the .rep carries no hypotheses, funding or
// publish PROP yet, and a QML UI module cannot read a file outside its own
// directory (CLAUDE.md platform constraint 1) — so the fixture travels with
// the UI as this singleton.
//
// GENERATED from fixtures/sample-project.json. Keep the two in step: when
// olb_core starts serving these sections, Main.qml swaps the binding to the
// backend PROP and nothing in the screens changes.
pragma Singleton
import QtQuick

QtObject {
    readonly property var hypotheses: ({
      "h1": {
        "id": "hyp-h1",
        "label": "H1",
        "statement": "Substitutions in the lid domain raise T50 by at least 5 °C relative to wild type without reducing specific activity below 80% of wild type.",
        "predictions": [
          {
            "label": "P1",
            "text": "At least 2 of 7 variants show ΔT50 ≥ 5 °C in DSF."
          },
          {
            "label": "P2",
            "text": "Those variants keep ≥ 80% of wild-type activity at 37 °C."
          }
        ],
        "falsification": "H1 is rejected if no variant reaches ΔT50 ≥ 5 °C across three replicates.",
        "analysisPlan": "Melting temperature from the first derivative of fluorescence; mean of three replicates; each variant compared with wild type by Welch's t-test, α = 0.05, Holm correction.",
        "growsFrom": {
          "recordId": "idea-lid",
          "label": "Idea · Deep mutational scan of the lid domain"
        },
        "review": {
          "summary": "Approved by Priya Raman and Ana Kovač · comments resolved",
          "approved": 2,
          "required": 2,
          "complete": true
        },
        "steps": [
          {
            "title": "Write",
            "note": "Done",
            "state": "done"
          },
          {
            "title": "Group review",
            "note": "2 of 2 approved",
            "state": "done"
          },
          {
            "title": "Commit on LEZ",
            "note": "Current step",
            "state": "current"
          },
          {
            "title": "Reveal at publication",
            "note": "Later",
            "state": "later"
          }
        ],
        "commitment": {
          "public": [
            "A salted fingerprint of this plan",
            "The time of commitment",
            "The project it belongs to"
          ],
          "private": [
            "The hypothesis, predictions and analysis plan",
            "Who wrote it",
            "The salt that unlocks the fingerprint"
          ],
          "fingerprintPreview": "sha256:9f2c41d8…07be41a",
          "sealedCopyForReviewers": true
        }
      }
    })

    readonly property var funding: ({
      "prop-lid": {
        "id": "prop-lid",
        "title": "Lid-domain screen",
        "badges": [
          {
            "kind": "done",
            "text": "Fully funded"
          },
          {
            "kind": "current",
            "text": "Tests H1"
          },
          {
            "kind": "neutral",
            "text": "Testnet tokens only"
          }
        ],
        "currency": "test tokens",
        "raised": "6,000",
        "raisedSub": "test tokens from 3 contributors",
        "released": "2,000",
        "releasedSub": "milestone M1",
        "escrow": "4,000",
        "escrowSub": "released only on approved evidence",
        "rule": "2 of 3 reviewers",
        "ruleSub": "enforced by the escrow program",
        "milestones": [
          {
            "id": "M1",
            "title": "Express and purify 7 variants and wild type",
            "amount": "2,000",
            "state": "released",
            "releasedOn": "9 Oct",
            "approvedBy": [
              "Ana Kovač",
              "Rui Santos"
            ],
            "evidence": [
              {
                "label": "Protocol v3",
                "recordId": "protocol-v3-dsf"
              },
              {
                "label": "Run 9"
              },
              {
                "label": "Run 12"
              }
            ]
          },
          {
            "id": "M2",
            "title": "Thermal shift on all variants, three replicates",
            "amount": "2,000",
            "state": "awaiting",
            "needed": 2,
            "evidence": [
              {
                "label": "Run 14",
                "recordId": "entry-run14"
              },
              {
                "label": "Dataset · DSF melt curves, runs 10–14"
              }
            ],
            "approvals": [
              {
                "name": "Ana Kovač",
                "state": "approved",
                "time": "1 h ago"
              },
              {
                "name": "Rui Santos",
                "state": "pending"
              },
              {
                "name": "Mei Tanaka (you)",
                "state": "pending",
                "you": true
              }
            ]
          },
          {
            "id": "M3",
            "title": "Activity at 37 °C and preprint",
            "amount": "2,000",
            "state": "locked"
          }
        ],
        "contributors": [
          {
            "name": "Open Science Collective",
            "kind": "public",
            "amount": "3,000"
          },
          {
            "name": "Host institution seed fund",
            "kind": "public",
            "amount": "1,500"
          },
          {
            "name": "Private contributor",
            "kind": "private",
            "amount": "1,500"
          }
        ],
        "reviewers": [
          {
            "name": "Ana Kovač",
            "key": "0x8c1e…44f0"
          },
          {
            "name": "Rui Santos",
            "key": "0x27ab…9d13"
          },
          {
            "name": "Mei Tanaka",
            "key": "0x5f90…c2e7"
          }
        ]
      }
    })

    readonly property var publish: ({
      "name": "Preprint bundle",
      "chain": [
        {
          "stage": "Idea",
          "title": "Deep mutational scan of the lid domain",
          "status": "Committed 3 Jul",
          "kind": "current"
        },
        {
          "stage": "Hypothesis",
          "title": "H1 · lid-domain stability",
          "status": "Committed 14 Aug",
          "kind": "current"
        },
        {
          "stage": "Funding",
          "title": "Lid-domain screen",
          "status": "3 milestones",
          "kind": "done"
        },
        {
          "stage": "Experiments",
          "title": "Runs 9–14",
          "status": "6 entries · 1 negative",
          "kind": "neutral"
        },
        {
          "stage": "Data",
          "title": "DSF melt curves, runs 10–14",
          "status": "RO-Crate · 12 files",
          "kind": "done"
        },
        {
          "stage": "Manuscript",
          "title": "Preprint v4",
          "status": "Ready to publish",
          "kind": "final"
        }
      ],
      "reveal": [
        {
          "id": "prereg",
          "title": "Reveal the H1 pre-registration",
          "note": "Fingerprint matches the 14 Aug commitment",
          "noteKind": "done",
          "checked": true
        },
        {
          "id": "idea",
          "title": "Reveal the original idea note",
          "note": "Optional · strengthens the priority claim",
          "noteKind": "neutral",
          "checked": false
        },
        {
          "id": "dataset",
          "title": "Make the DSF dataset public",
          "note": "Decrypts 12 files; their addresses become public",
          "noteKind": "neutral",
          "checked": true
        },
        {
          "id": "negative",
          "title": "Include the negative result from Run 13",
          "note": "Citable on its own, so nobody repeats it",
          "noteKind": "neutral",
          "checked": true
        },
        {
          "id": "raw",
          "title": "Include the raw instrument export (18.4 MB)",
          "note": "Only 1 copy exists · pin it to the replica before publishing",
          "noteKind": "attention",
          "checked": true,
          "copies": 1,
          "cid": "logos:zb2rh…9kLm",
          "action": "Pin to replica"
        }
      ],
      "credits": [
        {
          "name": "Priya Raman",
          "roles": "Conceptualization, Supervision",
          "work": "14 records",
          "status": "Accepted",
          "kind": "done"
        },
        {
          "name": "Arjun Mehta",
          "roles": "Investigation, Data curation",
          "work": "31 records",
          "status": "Accepted",
          "kind": "done"
        },
        {
          "name": "Lin Wei",
          "roles": "Investigation",
          "work": "9 records",
          "status": "Awaiting reply",
          "kind": "attention"
        },
        {
          "name": "Ana Kovač",
          "roles": "Review of funding milestones",
          "work": "2 approvals",
          "status": "Accepted",
          "kind": "done"
        },
        {
          "name": "Labbook agent",
          "note": "AI · run by Arjun Mehta",
          "roles": "Data curation",
          "work": "22 accepted edits",
          "status": "Listed separately",
          "kind": "neutral"
        }
      ],
      "manuscript": {
        "name": "lipase_lid_preprint_v4.pdf",
        "size": "2.1 MB",
        "role": "Manuscript"
      },
      "licence": "CC BY 4.0",
      "licences": [
        "CC BY 4.0",
        "CC0 1.0",
        "CC BY-SA 4.0"
      ],
      "after": [
        {
          "id": "review",
          "label": "Open public review on Logos Messaging",
          "checked": true
        },
        {
          "id": "rocrate",
          "label": "Export an RO-Crate copy for Zenodo or our repository",
          "checked": true
        },
        {
          "id": "reproduce",
          "label": "Invite reproduction of Run 14",
          "checked": false
        }
      ],
      "notice": {
        "kind": "attention",
        "text": "1 contributor has not accepted their credit record yet. You can publish now; their record stays open for 14 days."
      }
    })
}
