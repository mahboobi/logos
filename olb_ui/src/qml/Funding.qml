import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S4 · Funding and milestones (docs/03, design/canvas/Funding.dc.html).
//
// Escrowed testnet tokens, released a milestone at a time when reviewers
// approve the evidence. Rendered from `fundingData`, whose shape is
// fixtures/sample-project.json → funding["prop-lid"].
//
// The escrow itself is Phase 3: olb_funding is not deployed, so this screen
// never computes a release locally (docs/03 S4 rule) and never shows an
// approval it has not been told about. "Approve release" and "Ask for more
// evidence" are built for real and emit signals for the shell to wire.
Item {
    id: screen

    // ---- Interface contract ----
    property var fundingData: ({})
    signal approveRelease(string milestoneId)
    signal evidenceRequested(string milestoneId, string note)
    signal contributeRequested()
    signal openProposal()
    signal openEvidence(string milestoneId, string recordId, string label)
    signal openBoard()

    // ---- Derived, all guarded: the proposal can be empty or arrive late ----
    readonly property var proposal: screen.fundingData || ({})
    readonly property string proposalId: screen.proposal.id || ""
    readonly property string title: screen.proposal.title || ""
    readonly property var badges: screen.proposal.badges || []
    readonly property var milestones: screen.proposal.milestones || []
    readonly property var contributors: screen.proposal.contributors || []
    readonly property var reviewers: screen.proposal.reviewers || []
    readonly property bool twoColumn: screen.width > Theme.narrowBreakpoint

    // Badges may be plain strings (an older fixture) or { kind, text }.
    function badgeText(b) { return typeof b === "string" ? b : (b.text || ""); }
    function badgeKind(b) { return typeof b === "string" ? "neutral" : (b.kind || "neutral"); }

    // Evidence links may be plain labels or { label, recordId }.
    function evidenceLabel(e) { return typeof e === "string" ? e : (e.label || ""); }
    function evidenceRecord(e) { return typeof e === "string" ? "" : (e.recordId || ""); }

    function approvedCount(m) {
        var list = m.approvals || [];
        var n = 0;
        for (var i = 0; i < list.length; i++) if (list[i].state === "approved") n++;
        return n;
    }
    // docs/03 S4: the buttons appear only for a reviewer who has not acted.
    function viewerCanAct(m) {
        var list = m.approvals || [];
        for (var i = 0; i < list.length; i++)
            if (list[i].you === true && list[i].state !== "approved") return true;
        return false;
    }
    function lockedUntil(index) {
        if (index <= 0) return "the proposal opens";
        var prev = screen.milestones[index - 1];
        return (prev.id || "the previous milestone") + " is released";
    }

    // ---- One escrow summary card: mono metric over a meta sub-line ----
    component SummaryCard: Rectangle {
        id: sumCard
        property string label: ""
        property string value: ""
        property string sub: ""
        property bool mono: true

        implicitHeight: sumCol.implicitHeight + Theme.s14 * 2
        radius: Theme.rPanel
        color: Theme.surface
        border.width: 1
        border.color: Theme.line

        Accessible.role: Accessible.StaticText
        Accessible.name: sumCard.label + ": " + sumCard.value + ". " + sumCard.sub

        ColumnLayout {
            id: sumCol
            anchors.fill: parent
            anchors.margins: Theme.s14
            anchors.leftMargin: Theme.s16
            anchors.rightMargin: Theme.s16
            spacing: Theme.s4

            Text {
                text: sumCard.label
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Text {
                text: sumCard.value
                font.family: sumCard.mono ? Theme.mono : Theme.sans
                font.pixelSize: Theme.metric
                font.weight: Theme.semibold
                color: Theme.ink
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Text {
                visible: sumCard.sub !== ""
                text: sumCard.sub
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink2
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }
    }

    // ---- The "Evidence · a · b · c" line ----
    component EvidenceRow: Flow {
        id: evRow
        property string milestoneId: ""
        property var items: []

        spacing: Theme.s8

        FieldLabel {
            text: "Evidence"
            height: Theme.touchTarget
            verticalAlignment: Text.AlignVCenter
        }
        Repeater {
            model: evRow.items
            delegate: RowLayout {
                required property var modelData
                required property int index
                spacing: Theme.s8

                Text {
                    visible: index > 0
                    text: "·"
                    font.family: Theme.sans
                    font.pixelSize: Theme.body
                    color: Theme.dashed
                    Accessible.ignored: true
                }
                TextLink {
                    label: screen.evidenceLabel(modelData)
                    onClicked: screen.openEvidence(evRow.milestoneId,
                                                   screen.evidenceRecord(modelData),
                                                   screen.evidenceLabel(modelData))
                }
            }
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: screen.width
            spacing: Theme.s20

            // ---- Breadcrumb ----
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.topMargin: Theme.s28
                spacing: Theme.s6

                TextLink {
                    label: "Board"
                    size: Theme.small
                    weight: Theme.regular
                    compact: true
                    onClicked: screen.openBoard()
                }
                Text {
                    text: "/"
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink3
                    Accessible.ignored: true
                }
                Text {
                    text: "Funding"
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink3
                }
                Text {
                    text: "/"
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink3
                    Accessible.ignored: true
                }
                Text {
                    text: screen.title
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink
                }
                Item { Layout.fillWidth: true }
            }

            // ---- Header: title, badges, View proposal ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                columns: screen.twoColumn ? 2 : 1
                columnSpacing: Theme.s16
                rowSpacing: Theme.s16

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignBottom
                    spacing: Theme.s8

                    Text {
                        text: screen.title
                        font.family: Theme.sans
                        font.pixelSize: Theme.pageTitle
                        font.weight: Theme.semibold
                        font.letterSpacing: Theme.titleSpacing
                        color: Theme.ink
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: Theme.s6

                        Repeater {
                            model: screen.badges
                            delegate: Badge {
                                required property var modelData
                                text: screen.badgeText(modelData)
                                kind: screen.badgeKind(modelData)
                            }
                        }
                    }
                }

                PrimaryButton {
                    text: "View proposal"
                    primary: false
                    Layout.alignment: screen.twoColumn ? Qt.AlignRight | Qt.AlignBottom
                                                       : Qt.AlignLeft
                    onClicked: screen.openProposal()
                }
            }

            // ---- Escrow summary ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                columns: Math.max(1, Math.min(4, Math.floor((screen.width - Theme.s32 * 2) / 230)))
                columnSpacing: Theme.s12
                rowSpacing: Theme.s12

                Accessible.role: Accessible.Grouping
                Accessible.name: "Escrow summary"

                SummaryCard {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    label: "Raised"
                    value: screen.proposal.raised || ""
                    sub: screen.proposal.raisedSub || ""
                }
                SummaryCard {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    label: "Released to the project"
                    value: screen.proposal.released || ""
                    sub: screen.proposal.releasedSub || ""
                }
                SummaryCard {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    label: "Held in escrow"
                    value: screen.proposal.escrow || ""
                    sub: screen.proposal.escrowSub || ""
                }
                SummaryCard {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    label: "Release rule"
                    value: screen.proposal.rule || ""
                    sub: screen.proposal.ruleSub || ""
                    mono: false
                }
            }

            // ---- Milestones, contributors and reviewers ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s48
                columns: screen.twoColumn ? 2 : 1
                columnSpacing: Theme.s20
                rowSpacing: Theme.s20

                // ================= Milestones =================
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s12

                    Text {
                        text: "Milestones"
                        font.family: Theme.sans
                        font.pixelSize: Theme.sectionTitle
                        font.weight: Theme.semibold
                        color: Theme.ink
                        Layout.fillWidth: true
                    }

                    Repeater {
                        model: screen.milestones

                        delegate: Rectangle {
                            id: card
                            required property var modelData
                            required property int index

                            readonly property string msState: modelData.state || "locked"
                            readonly property bool awaiting: card.msState === "awaiting"
                            readonly property bool released: card.msState === "released"
                            readonly property bool locked: card.msState === "locked"
                            readonly property var approvals: modelData.approvals || []
                            readonly property var evidence: modelData.evidence || []
                            readonly property int approved: screen.approvedCount(modelData)
                            readonly property int needed: modelData.needed || 0

                            Layout.fillWidth: true
                            implicitHeight: cardCol.implicitHeight
                                            + (card.awaiting ? Theme.s20 - 1 : Theme.s20) * 2
                            radius: Theme.rPanel
                            color: Theme.surface
                            // Amber 2 px only while the milestone needs a decision.
                            border.width: card.awaiting ? 2 : 1
                            border.color: card.awaiting ? Theme.amber : Theme.line

                            ColumnLayout {
                                id: cardCol
                                anchors.fill: parent
                                anchors.margins: card.awaiting ? Theme.s20 - 1 : Theme.s20
                                spacing: Theme.s12

                                // ---- Title and amount ----
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s8

                                    Text {
                                        text: (modelData.id || "") + " · " + (modelData.title || "")
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.lede
                                        font.weight: Theme.semibold
                                        color: card.locked ? Theme.ink2 : Theme.ink
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        text: modelData.amount || ""
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.body
                                        font.weight: Theme.medium
                                        color: card.locked ? Theme.ink2 : Theme.ink
                                        Layout.alignment: Qt.AlignTop
                                    }
                                }

                                // ---- State line ----
                                Flow {
                                    Layout.fillWidth: true
                                    spacing: Theme.s6

                                    Badge {
                                        visible: card.released
                                        text: "Released " + (modelData.releasedOn || "")
                                        kind: "done"
                                    }
                                    Badge {
                                        visible: card.awaiting
                                        text: "Awaiting approval · " + card.approved
                                              + " of " + card.needed + " needed"
                                        kind: "attention"
                                    }
                                    Badge {
                                        visible: card.locked
                                        text: "Locked until " + screen.lockedUntil(card.index)
                                        kind: "neutral"
                                    }
                                    Text {
                                        visible: card.released && (modelData.approvedBy || []).length > 0
                                        text: "Approved by " + (modelData.approvedBy || []).join(", ")
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                        height: Theme.chipHeight
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                    Text {
                                        visible: card.locked && card.evidence.length === 0
                                        text: "No evidence yet"
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                        height: Theme.chipHeight
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }

                                // ---- Evidence ----
                                EvidenceRow {
                                    visible: card.evidence.length > 0
                                    milestoneId: modelData.id || ""
                                    items: card.evidence
                                    Layout.fillWidth: true
                                }

                                // ---- Who has approved ----
                                Rectangle {
                                    visible: card.approvals.length > 0
                                    Layout.fillWidth: true
                                    Layout.topMargin: Theme.s2
                                    Layout.preferredHeight: 1
                                    color: Theme.lineSoft
                                }
                                ColumnLayout {
                                    visible: card.approvals.length > 0
                                    Layout.fillWidth: true
                                    spacing: Theme.s8

                                    Repeater {
                                        model: card.approvals
                                        delegate: RowLayout {
                                            required property var modelData
                                            readonly property bool ok: modelData.state === "approved"
                                            Layout.fillWidth: true
                                            spacing: Theme.s8

                                            Accessible.role: Accessible.StaticText
                                            Accessible.name: (modelData.name || "") + ": "
                                                + (ok ? "approved" : "not yet reviewed")

                                            Text {
                                                text: modelData.name || ""
                                                font.family: Theme.sans
                                                font.pixelSize: Theme.body
                                                color: Theme.ink
                                                wrapMode: Text.Wrap
                                                Layout.fillWidth: true
                                            }
                                            Text {
                                                text: ok
                                                    ? "Approved" + (modelData.time
                                                        ? " · " + modelData.time : "")
                                                    : "Not yet reviewed"
                                                font.family: Theme.sans
                                                font.pixelSize: Theme.meta
                                                font.weight: ok ? Theme.medium : Theme.regular
                                                color: ok ? Theme.accent : Theme.ink3
                                            }
                                        }
                                    }
                                }

                                // ---- Reviewer actions ----
                                Flow {
                                    visible: card.awaiting && screen.viewerCanAct(modelData)
                                    Layout.fillWidth: true
                                    spacing: Theme.s8

                                    PrimaryButton {
                                        text: "Approve release"
                                        accessibleName: "Approve release of " + (modelData.id || "")
                                        onClicked: screen.approveRelease(modelData.id || "")
                                    }
                                    PrimaryButton {
                                        text: "Ask for more evidence"
                                        primary: false
                                        accessibleName: "Ask for more evidence on "
                                                        + (modelData.id || "")
                                        onClicked: screen.evidenceRequested(modelData.id || "", "")
                                    }
                                }
                            }
                        }
                    }
                }

                // ================= Side column =================
                ColumnLayout {
                    Layout.fillWidth: !screen.twoColumn
                    Layout.preferredWidth: screen.twoColumn ? 320 : -1
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s16

                    // ---- Contributors ----
                    Panel {
                        title: "Contributors"
                        Layout.fillWidth: true

                        Repeater {
                            model: screen.contributors
                            delegate: RowLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: Theme.s8

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0

                                    Text {
                                        text: modelData.name || ""
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.body
                                        font.weight: Theme.medium
                                        color: Theme.ink
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        text: modelData.kind === "private"
                                              ? "Identity hidden · private LEZ account"
                                              : "Public contribution"
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                                Text {
                                    text: modelData.amount || ""
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.body
                                    color: Theme.ink
                                    Layout.alignment: Qt.AlignTop
                                }
                            }
                        }

                        PrimaryButton {
                            text: "Contribute"
                            primary: false
                            Layout.fillWidth: true
                            Layout.topMargin: Theme.s4
                            onClicked: screen.contributeRequested()
                        }
                    }

                    // ---- Reviewers ----
                    Panel {
                        title: "Reviewers"
                        Layout.fillWidth: true

                        Repeater {
                            model: screen.reviewers
                            delegate: ColumnLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 0

                                Text {
                                    text: modelData.name || ""
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.body
                                    font.weight: Theme.medium
                                    color: Theme.ink
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: "key " + (modelData.key || "")
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                    wrapMode: Text.WrapAnywhere
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.topMargin: Theme.s10
                            Layout.preferredHeight: 1
                            color: Theme.lineSoft
                        }
                        Text {
                            text: "Released funds go to the project's shared account. No "
                                  + "single key can move escrowed funds; the program checks "
                                  + "reviewer approvals first."
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            lineHeight: Theme.bodyLineHeight
                            color: Theme.ink2
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }
    }
}
