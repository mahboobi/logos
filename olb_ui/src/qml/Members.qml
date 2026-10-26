import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S7 · Project members, with M1 · Invite a member.
//
// Spec:    docs/03-screens.md, "S7 · Project members" and "M1 · Invite a member (modal)"
// Design:  design/canvas/Members.dc.html, InviteDialog/InviteCode/InviteStep2-4.dc.html
// Crypto:  docs/04-invite-flow.md (identity = OLB Ed25519 signing key + ML-KEM-768
//          sealing key; transport = a private Logos Delivery inbox topic). A Logos
//          Chat address is never an identity and membership is not a Chat group, so
//          no copy here says otherwise — docs/03 M1 step 3 already replaces the
//          design's Chat wording, and that replacement is what is used below.
//
// The M1 modal is declared inside this file (`inviteModal`) rather than as its own
// component file, so it can read this screen's props directly.
Item {
    id: screen

    // ---- interface contract -------------------------------------------------
    property var backend: null                 // may be null; every use is guarded
    property var members: ({})                 // { people, replicas, pendingInvites, projectKey }
    property var inviteDemo: ({})              // { olbId, parsed{...}, code }
    signal inviteRequested(string olbId, string role)
    signal memberRemoved(string memberId)
    signal keyRotationRequested()

    // ---- read-only views of the props --------------------------------------
    readonly property var people: screen.members.people || []
    readonly property var replicas: screen.members.replicas || []
    readonly property var pendingInvites: screen.members.pendingInvites || []
    readonly property var projectKey: screen.members.projectKey || ({})

    // Optional extras. olb_core exposes neither yet, so they are read if the host
    // happens to supply them and stay empty otherwise.
    // TODO(phase1c): project.get(id).title — for the header meta line.
    readonly property string projectName: screen.members.projectName || "Project"
    // TODO(phase1c): project.ideas() — for the M1 "Selected ideas" checklist.
    readonly property var accessIdeas: screen.members.ideas || []

    // ---- helpers ------------------------------------------------------------
    // Design colour mapping from Members.dc.html: Lead green, Contributor violet,
    // Reviewer slate.
    function roleKind(role) {
        var r = String(role || "").toLowerCase();
        if (r.indexOf("lead") === 0) return "done";
        if (r.indexOf("contributor") === 0) return "current";
        return "neutral";
    }
    // "9C4F 2B71 D0A8 · 5E3C 11F9 E21A" -> "9C4F…E21A" (docs/03: show short, full on hover)
    function shortFingerprint(fp) {
        if (!fp) return "";
        var parts = String(fp).replace(/·/g, " ").split(/\s+/).filter(function (s) { return s.length > 0; });
        if (parts.length < 2) return String(fp);
        return parts[0] + "…" + parts[parts.length - 1];
    }
    function firstName(full) {
        var s = String(full || "").trim();
        if (s.length === 0) return "them";
        return s.split(/\s+/)[0];
    }

    // A row of exclusive pills with an ink fill when selected (docs/02, and the
    // same treatment NewProject.qml uses for team size).
    component PillRow: Flow {
        id: pillRow
        property var entries: []
        property string selected: ""
        property string groupName: ""
        signal picked(string label)
        spacing: Theme.s8

        Repeater {
            model: pillRow.entries
            delegate: Rectangle {
                required property string modelData
                readonly property bool active: pillRow.selected === modelData

                height: Theme.touchTarget
                width: pillText.implicitWidth + Theme.s16 * 2
                radius: Theme.rControl
                color: active ? Theme.ink : Theme.surface
                border.width: 1
                border.color: active ? Theme.ink : Theme.inputBorder

                Accessible.role: Accessible.RadioButton
                Accessible.name: pillRow.groupName === "" ? modelData : (pillRow.groupName + ", " + modelData)
                Accessible.checked: active
                Accessible.onPressAction: pillRow.picked(modelData)
                activeFocusOnTab: true

                Rectangle {                       // keyboard focus ring
                    anchors.fill: parent
                    anchors.margins: -3
                    radius: Theme.rControl
                    color: "transparent"
                    border.width: 2
                    border.color: Theme.violet
                    visible: parent.activeFocus
                }

                Text {
                    id: pillText
                    anchors.centerIn: parent
                    text: modelData
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    font.weight: Theme.medium
                    color: parent.active ? Theme.textOnAccent : Theme.ink2
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: pillRow.picked(modelData)
                }
                Keys.onReturnPressed: pillRow.picked(modelData)
                Keys.onSpacePressed: pillRow.picked(modelData)
            }
        }
    }

    // Hidden sink used to put text on the clipboard ("Copy code").
    TextEdit {
        id: clipboard
        visible: false
        width: 0; height: 0
    }
    function copyToClipboard(text) {
        clipboard.text = text;
        clipboard.selectAll();
        clipboard.copy();
        clipboard.deselect();
    }

    // =========================================================================
    // S7
    // =========================================================================
    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: screen.width
            spacing: Theme.s24

            // ---- PageHeader ----
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.topMargin: Theme.s32
                spacing: Theme.s8

                Text {
                    text: screen.projectName
                    textFormat: Text.PlainText
                    font.family: Theme.sans
                    font.pixelSize: Theme.overline
                    font.weight: Theme.medium
                    font.letterSpacing: Theme.overlineSpacing
                    color: Theme.ink3
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s16

                    Text {
                        text: "Members"
                        font.family: Theme.sans
                        font.pixelSize: Theme.pageTitle
                        font.weight: Theme.semibold
                        font.letterSpacing: Theme.titleSpacing
                        color: Theme.ink
                        Layout.fillWidth: true
                    }
                    PrimaryButton {
                        id: inviteButton
                        text: "Invite member"
                        implicitHeight: Theme.touchTarget
                        Accessible.name: "Invite member"
                        onClicked: inviteModal.openFresh()
                    }
                }

                Text {
                    text: "Everyone here holds the project key and can read its private records. "
                          + "What they can change depends on their role."
                    font.family: Theme.sans
                    font.pixelSize: Theme.lede
                    color: Theme.ink3
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                    Layout.maximumWidth: 720
                }
            }

            // ---- Panel: People (n) ----
            Panel {
                title: "People (" + screen.people.length + ")"
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32

                // Table head
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s10

                    Text {
                        text: "Member"; Layout.fillWidth: true
                        font.family: Theme.sans; font.pixelSize: Theme.overline
                        font.weight: Theme.medium; font.letterSpacing: Theme.overlineSpacing
                        color: Theme.ink3
                    }
                    Text {
                        text: "Role"; Layout.preferredWidth: 140
                        font.family: Theme.sans; font.pixelSize: Theme.overline
                        font.weight: Theme.medium; font.letterSpacing: Theme.overlineSpacing
                        color: Theme.ink3
                    }
                    Text {
                        text: "Ideas"; Layout.preferredWidth: 70
                        font.family: Theme.sans; font.pixelSize: Theme.overline
                        font.weight: Theme.medium; font.letterSpacing: Theme.overlineSpacing
                        color: Theme.ink3
                    }
                    Text {
                        text: "Signing key"; Layout.preferredWidth: 130
                        font.family: Theme.sans; font.pixelSize: Theme.overline
                        font.weight: Theme.medium; font.letterSpacing: Theme.overlineSpacing
                        color: Theme.ink3
                    }
                    Text {
                        text: "Status"; Layout.preferredWidth: 150
                        font.family: Theme.sans; font.pixelSize: Theme.overline
                        font.weight: Theme.medium; font.letterSpacing: Theme.overlineSpacing
                        color: Theme.ink3
                    }
                    Item { Layout.preferredWidth: 100 }
                }
                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.line }

                Repeater {
                    model: screen.people
                    delegate: ColumnLayout {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        spacing: Theme.s12

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s10

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s2
                                Text {
                                    // Display names are untrusted input (docs/04 §9) — plain text only.
                                    text: modelData.name || ""
                                    textFormat: Text.PlainText
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.body
                                    font.weight: Theme.medium
                                    color: Theme.ink
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Text {
                                    // Amber when ORCID is not linked: unverified needs attention.
                                    readonly property bool unlinked: String(modelData.note || "").indexOf("not linked") >= 0
                                    text: modelData.note || ""
                                    textFormat: Text.PlainText
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.meta
                                    color: unlinked ? Theme.amber : Theme.ink3
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }

                            Badge {
                                Layout.preferredWidth: 140
                                Layout.maximumWidth: 140
                                text: modelData.role || ""
                                kind: screen.roleKind(modelData.role)
                            }
                            Text {
                                text: modelData.ideas || ""
                                Layout.preferredWidth: 70
                                font.family: Theme.sans; font.pixelSize: Theme.small
                                color: Theme.ink2
                            }
                            Text {
                                // Keys and fingerprints: mono, violet (CLAUDE.md UI rules).
                                text: modelData.key || ""
                                Layout.preferredWidth: 130
                                font.family: Theme.mono; font.pixelSize: Theme.meta
                                color: Theme.violet
                                elide: Text.ElideRight
                                ToolTip.visible: keyHover.hovered
                                ToolTip.text: modelData.key || ""
                                HoverHandler { id: keyHover }
                            }
                            RowLayout {
                                Layout.preferredWidth: 150
                                spacing: Theme.s6
                                Rectangle {
                                    width: Theme.statusDot; height: Theme.statusDot
                                    radius: Theme.statusDot / 2
                                    color: modelData.online ? Theme.accent : Theme.dashed
                                }
                                Text {
                                    text: modelData.online ? "Online"
                                                           : ("Last seen " + (modelData.lastSeen || ""))
                                    textFormat: Text.PlainText
                                    font.family: Theme.sans; font.pixelSize: Theme.small
                                    color: modelData.online ? Theme.ink : Theme.ink3
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }
                            Button {
                                text: "Manage"
                                Layout.preferredWidth: 100
                                implicitHeight: Theme.touchTarget
                                Accessible.name: "Manage " + (modelData.name || "member")
                                onClicked: manageSheet.openFor(modelData)
                            }
                        }

                        Rectangle {
                            visible: index < screen.people.length - 1
                            Layout.fillWidth: true
                            Layout.preferredHeight: 1
                            color: Theme.lineSoft
                        }
                    }
                }

                Rectangle {                     // empty state
                    visible: screen.people.length === 0
                    Layout.fillWidth: true
                    Layout.preferredHeight: 88
                    radius: Theme.rPanel
                    color: "transparent"
                    border.width: 1
                    border.color: Theme.dashed
                    Text {
                        anchors.centerIn: parent
                        text: "Only you so far. Invite a member to share the project key."
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        color: Theme.ink3
                    }
                }
            }

            // ---- Two columns: role matrix | replicas, invites, key ----
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s32
                Layout.alignment: Qt.AlignTop
                spacing: Theme.s20

                // ---- Panel: What each role can do ----
                Panel {
                    id: roleMatrix
                    title: "What each role can do"
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop

                    readonly property var rows: [
                        { p: "Read private records",            lead: "Yes", contributor: "Yes",      reviewer: "Yes" },
                        { p: "Write entries and open branches",  lead: "Yes", contributor: "Yes",      reviewer: "No"  },
                        { p: "Approve merges into main",         lead: "Yes", contributor: "Yes",      reviewer: "No"  },
                        { p: "Commit hypotheses on LEZ",         lead: "Yes", contributor: "Proposes", reviewer: "No"  },
                        { p: "Approve funding milestones",       lead: "No",  contributor: "No",       reviewer: "Yes" },
                        { p: "Publish and invite members",       lead: "Yes", contributor: "No",       reviewer: "No"  }
                    ]

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s10
                        Text {
                            text: "Permission"; Layout.fillWidth: true
                            font.family: Theme.sans; font.pixelSize: Theme.overline
                            font.weight: Theme.medium; font.letterSpacing: Theme.overlineSpacing
                            color: Theme.ink3
                        }
                        Repeater {
                            model: ["Lead", "Contributor", "Reviewer"]
                            delegate: Text {
                                required property string modelData
                                text: modelData
                                Layout.preferredWidth: 90
                                font.family: Theme.sans; font.pixelSize: Theme.overline
                                font.weight: Theme.medium; font.letterSpacing: Theme.overlineSpacing
                                color: Theme.ink3
                            }
                        }
                    }
                    Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.line }

                    Repeater {
                        model: roleMatrix.rows
                        delegate: ColumnLayout {
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            spacing: Theme.s10

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s10
                                Text {
                                    text: modelData.p
                                    Layout.fillWidth: true
                                    font.family: Theme.sans; font.pixelSize: Theme.small
                                    color: Theme.ink2
                                    wrapMode: Text.Wrap
                                }
                                Repeater {
                                    model: [modelData.lead, modelData.contributor, modelData.reviewer]
                                    delegate: Text {
                                        required property string modelData
                                        readonly property bool yes: modelData === "Yes"
                                        text: modelData
                                        Layout.preferredWidth: 90
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.small
                                        font.weight: yes ? Theme.medium : Theme.regular
                                        color: yes ? Theme.accent : Theme.ink3
                                    }
                                }
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: Theme.lineSoft
                            }
                        }
                    }

                    Text {
                        text: "Researchers never approve funding for their own work; that stays with reviewers."
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        color: Theme.ink3
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }

                // ---- Side column ----
                ColumnLayout {
                    Layout.preferredWidth: 340
                    Layout.maximumWidth: 340
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s16

                    // ---- Panel: Replica nodes ----
                    Panel {
                        title: "Replica nodes"
                        Layout.fillWidth: true

                        Repeater {
                            model: screen.replicas
                            delegate: ColumnLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: Theme.s2
                                Text {
                                    text: modelData.name || ""
                                    textFormat: Text.PlainText
                                    font.family: Theme.sans; font.pixelSize: Theme.small
                                    font.weight: Theme.medium
                                    color: Theme.ink
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: modelData.meta || ""
                                    textFormat: Text.PlainText
                                    font.family: Theme.sans; font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        Button {
                            text: "Add a replica node"
                            implicitHeight: Theme.touchTarget
                            Layout.fillWidth: true
                            Accessible.name: "Add a replica node"
                            // TODO(phase1c): replicas.add(addressOrId) — no core method and no
                            // signal in this screen's contract, so the control is inert for now.
                            onClicked: {}
                        }
                    }

                    // ---- Panel: Pending invites ----
                    Panel {
                        title: "Pending invites"
                        Layout.fillWidth: true

                        Repeater {
                            model: screen.pendingInvites
                            delegate: ColumnLayout {
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true
                                spacing: Theme.s12

                                readonly property bool byCode: (modelData.kind === "code")
                                readonly property bool awaiting: (modelData.state === "awaiting_approval")

                                Rectangle {
                                    visible: index > 0
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 1
                                    color: Theme.lineSoft
                                }

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s8

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: Theme.s2

                                        // Sent by ID: their name. Redeemed code: the code itself.
                                        Text {
                                            visible: !byCode
                                            text: modelData.name || ""
                                            textFormat: Text.PlainText
                                            font.family: Theme.sans; font.pixelSize: Theme.small
                                            font.weight: Theme.medium
                                            color: Theme.ink
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        RowLayout {
                                            visible: byCode
                                            Layout.fillWidth: true
                                            spacing: Theme.s6
                                            Text {
                                                text: "Code"
                                                font.family: Theme.sans; font.pixelSize: Theme.small
                                                font.weight: Theme.medium
                                                color: Theme.ink
                                            }
                                            Text {
                                                text: modelData.code || ""
                                                font.family: Theme.mono; font.pixelSize: Theme.small
                                                font.weight: Theme.medium
                                                color: Theme.violet
                                                elide: Text.ElideRight
                                                Layout.fillWidth: true
                                            }
                                        }

                                        Text {
                                            text: byCode
                                                  ? ("Redeemed by " + (modelData.requester || "") + " · key "
                                                     + screen.shortFingerprint(modelData.fingerprint))
                                                  : ((modelData.role || "") + " · key "
                                                     + screen.shortFingerprint(modelData.fingerprint)
                                                     + " · sent " + (modelData.sent || ""))
                                            textFormat: Text.PlainText
                                            font.family: Theme.sans; font.pixelSize: Theme.meta
                                            color: Theme.ink3
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                            ToolTip.visible: fpHover.hovered && !!modelData.fingerprint
                                            ToolTip.text: modelData.fingerprint || ""
                                            HoverHandler { id: fpHover }
                                        }

                                        // Amber: waiting on their node. Violet: waiting on you.
                                        Text {
                                            text: awaiting ? "Waiting for your approval"
                                                           : "Not yet received by their node"
                                            font.family: Theme.sans; font.pixelSize: Theme.meta
                                            font.weight: Theme.medium
                                            color: awaiting ? Theme.violet : Theme.amber
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                    }

                                    PrimaryButton {
                                        visible: awaiting
                                        text: "Review"
                                        implicitHeight: Theme.touchTarget
                                        Accessible.name: "Review the request from " + (modelData.requester || "a new node")
                                        onClicked: inviteModal.openReview(modelData)
                                    }
                                    Button {
                                        visible: !awaiting
                                        text: "Resend"
                                        implicitHeight: Theme.touchTarget
                                        Accessible.name: "Resend the invite to " + (modelData.name || "them")
                                        // TODO(phase1c): invites.resend(inviteId) — re-publishes the same
                                        // sealed envelope to their inbox topic (docs/04 §4). No core
                                        // method and no contract signal for it yet.
                                        onClicked: {}
                                    }
                                }
                            }
                        }

                        Text {
                            visible: screen.pendingInvites.length === 0
                            text: "No invites in flight."
                            font.family: Theme.sans; font.pixelSize: Theme.small
                            color: Theme.ink3
                            Layout.fillWidth: true
                        }
                    }

                    // ---- Panel: Project key ----
                    Panel {
                        title: "Project key"
                        Layout.fillWidth: true

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s6
                            Text {
                                text: "Version " + (screen.projectKey.version !== undefined
                                                    ? screen.projectKey.version : "—")
                                font.family: Theme.sans; font.pixelSize: Theme.small
                                font.weight: Theme.medium
                                color: Theme.violet
                            }
                            Text {
                                text: "· last rotated " + (screen.projectKey.rotated || "—")
                                textFormat: Text.PlainText
                                font.family: Theme.sans; font.pixelSize: Theme.small
                                color: Theme.ink3
                                Layout.fillWidth: true
                            }
                        }
                        Text {
                            text: "Removing a member rotates the key. They keep what they already had, "
                                  + "but cannot read anything new."
                            font.family: Theme.sans; font.pixelSize: Theme.meta
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }
    }

    // =========================================================================
    // Manage sheet — docs/03 S7: "opens a role/access/remove sheet — to be
    // designed; use a simple dialog".
    // =========================================================================
    Popup {
        id: manageSheet
        property var person: null

        width: 420
        height: Math.min(implicitHeight, screen.height - Theme.s48)
        x: Math.round((screen.width - width) / 2)
        y: Math.max(Theme.s24, Math.round((screen.height - height) / 2))
        modal: true
        padding: Theme.s20
        closePolicy: Popup.CloseOnEscape

        Overlay.modal: Rectangle { color: Theme.backdrop }

        background: Rectangle {
            radius: Theme.rModal
            color: Theme.surface
            border.width: 1
            border.color: Theme.line
        }

        function openFor(p) { manageSheet.person = p; manageSheet.open(); }
        onClosed: { manageSheet.person = null; inviteButton.forceActiveFocus(); }

        contentItem: ColumnLayout {
            spacing: Theme.s12

            Text {
                text: "Manage " + ((manageSheet.person && manageSheet.person.name) || "member")
                textFormat: Text.PlainText
                font.family: Theme.sans
                font.pixelSize: Theme.modalTitle
                font.weight: Theme.semibold
                color: Theme.ink
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.s8
                Badge {
                    text: (manageSheet.person && manageSheet.person.role) || ""
                    kind: screen.roleKind(manageSheet.person && manageSheet.person.role)
                }
                Text {
                    text: ((manageSheet.person && manageSheet.person.ideas) || "") + " ideas"
                    font.family: Theme.sans; font.pixelSize: Theme.small
                    color: Theme.ink2
                    Layout.fillWidth: true
                }
            }

            FieldLabel { text: "Signing key" }
            Text {
                text: (manageSheet.person && manageSheet.person.key) || ""
                font.family: Theme.mono; font.pixelSize: Theme.fingerprint
                color: Theme.violet
                Layout.fillWidth: true
            }

            // TODO(phase1c): members.setRole(memberId, role) and
            // members.grantIdeaAccess(memberId, ideaIds) — the role and access half
            // of this sheet is left out rather than faked, because changing either
            // is a signed commit that only olb_core can write (docs/04 §6).

            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.lineSoft }

            Text {
                text: "Removing a member rotates the key. They keep what they already had, "
                      + "but cannot read anything new."
                font.family: Theme.sans; font.pixelSize: Theme.meta
                color: Theme.ink3
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.s8
                Item { Layout.fillWidth: true }
                Button {
                    text: "Cancel"
                    implicitHeight: Theme.touchTarget
                    Accessible.name: "Cancel"
                    onClicked: manageSheet.close()
                }
                Button {
                    text: "Remove from project"
                    implicitHeight: Theme.touchTarget
                    Accessible.name: "Remove from project"
                    onClicked: {
                        var p = manageSheet.person || ({});
                        // The fixture's people carry no id, so fall back to the name.
                        // TODO(phase1c): members.list() should return a stable memberId.
                        screen.memberRemoved(p.id || p.name || "");
                        // docs/04 §7: removal rotates the project key for everyone left.
                        screen.keyRotationRequested();
                        manageSheet.close();
                    }
                }
            }
        }
    }

    // =========================================================================
    // M1 · Invite a member. Four states: 1 Identify (ID or code), 2 Role and
    // access, 3 Review and send, 4 Sent.
    // =========================================================================
    Popup {
        id: inviteModal

        // ---- modal state ----
        property int step: 1
        property string method: "id"                 // "id" | "code"
        property string role: "Contributor"
        property string access: "All ideas"
        property var pickedIdeas: ({})
        property string expiry: "48 hours"
        property string uses: "Once"
        property bool fingerprintChecked: false
        property bool confirmDiscard: false
        // Opened from "Review" on a redeemed code: step 3 with an Approve primary.
        property bool reviewMode: false
        property var reviewInvite: null

        readonly property bool isCode: inviteModal.method === "code"
        // TODO(phase1c): identity.parseOlbId(olbId) -> { name, orcidClaimed,
        // fingerprint, online }. Until core parses and verifies the self-signature,
        // the preview is rendered from `inviteDemo.parsed`.
        readonly property var parsed: screen.inviteDemo.parsed || ({})
        readonly property string idText: idField.text
        readonly property bool idValid: {
            var t = inviteModal.idText.trim();
            return t.indexOf("olb1:") === 0 && t.length > 12;
        }
        readonly property string inviteeName: inviteModal.reviewMode
            ? ((inviteModal.reviewInvite && inviteModal.reviewInvite.requester) || "")
            : (inviteModal.parsed.name || "")
        readonly property string code: screen.inviteDemo.code || ""
        readonly property int ideaCount: screen.accessIdeas.length
        readonly property int pickedCount: {
            var n = 0;
            for (var k in inviteModal.pickedIdeas) if (inviteModal.pickedIdeas[k]) n++;
            return n;
        }
        readonly property string accessLabel: {
            if (inviteModal.access === "All ideas" || inviteModal.ideaCount === 0) {
                return inviteModal.ideaCount > 0 ? ("All " + inviteModal.ideaCount + " ideas") : "All ideas";
            }
            return inviteModal.pickedCount + " of " + inviteModal.ideaCount + " ideas";
        }
        readonly property bool canContinue: {
            if (inviteModal.step !== 1) return true;
            if (inviteModal.isCode) return inviteModal.code.length > 0;
            return inviteModal.idValid && inviteModal.fingerprintChecked;
        }
        readonly property string primaryLabel: {
            if (inviteModal.step === 4) return "Done";
            if (inviteModal.step === 3) return inviteModal.reviewMode ? "Approve"
                                             : (inviteModal.isCode ? "Activate code" : "Send invite");
            return "Continue";
        }

        readonly property var stepLabels: ["Identify", "Role and access", "Review and send"]

        readonly property var summaryRows: {
            if (inviteModal.reviewMode) {
                var inv = inviteModal.reviewInvite || ({});
                return [
                    { k: "Member",          v: inv.requester || "" },
                    { k: "Key fingerprint", v: inv.fingerprint || "" },
                    { k: "Role",            v: inv.role || inviteModal.role },
                    { k: "Access",          v: inviteModal.accessLabel }
                ];
            }
            if (inviteModal.isCode) {
                return [
                    { k: "Invite",    v: "Code " + inviteModal.code },
                    { k: "Valid for", v: inviteModal.uses + " · " + inviteModal.expiry },
                    { k: "Role",      v: inviteModal.role },
                    { k: "Access",    v: inviteModal.accessLabel }
                ];
            }
            return [
                { k: "Member",          v: inviteModal.parsed.name || "" },
                { k: "Key fingerprint", v: screen.shortFingerprint(inviteModal.parsed.fingerprint) },
                { k: "Role",            v: inviteModal.role },
                { k: "Access",          v: inviteModal.accessLabel }
            ];
        }

        readonly property var happensRows: {
            var who = inviteModal.inviteeName;
            if (inviteModal.isCode && !inviteModal.reviewMode) {
                return ["The code becomes active. Share it with them yourself.",
                        "When they redeem it, their node sends you its sealing key.",
                        "You approve the request in Members after checking their fingerprint.",
                        "Only then is the project key sealed to them and sent."];
            }
            // docs/03 M1 step 3, "Copy change for implementation": the envelope goes
            // over a Logos Delivery inbox topic, not a Chat message.
            return ["The project key is sealed to " + who + "’s key. Only " + who + "’s node can open it.",
                    "It travels end-to-end encrypted over Logos Messaging to their inbox.",
                    who + " shows as pending in Members until their node confirms.",
                    "A signed commit, Add " + who + " as " + inviteModal.role + ", goes into History."];
        }

        readonly property var progressRows: {
            if (inviteModal.reviewMode) {
                return [{ l: "Invite code created",              s: "done" },
                        { l: "Code redeemed by their node",       s: "done" },
                        { l: "You approve their request",         s: "done" },
                        { l: "Project key sealed and delivered",  s: "now"  }];
            }
            if (inviteModal.isCode) {
                return [{ l: "Invite code created",              s: "done"  },
                        { l: "Code redeemed by their node",       s: "now"   },
                        { l: "You approve their request",         s: "later" },
                        { l: "Project key sealed and delivered",  s: "later" }];
            }
            var who = inviteModal.inviteeName;
            return [{ l: "Project key sealed to " + who + "’s key", s: "done"  },
                    { l: "Handed to Logos Delivery",                s: "done"  },
                    { l: "Received by " + who + "’s node",          s: "now"   },
                    { l: "Accepted, key installed",                 s: "later" }];
        }

        readonly property string doneTitle: (inviteModal.isCode && !inviteModal.reviewMode)
            ? "Invite code is active"
            : ("Invite sent to " + inviteModal.inviteeName)
        readonly property string doneSub: (inviteModal.isCode && !inviteModal.reviewMode)
            ? ("Share " + inviteModal.code + " with them. It expires in " + inviteModal.expiry + ".")
            : ("They join as " + inviteModal.role + " with access to "
               + inviteModal.accessLabel.toLowerCase() + ".")

        // ---- behaviour ----
        function reset() {
            inviteModal.step = 1;
            inviteModal.method = "id";
            inviteModal.role = "Contributor";
            inviteModal.access = "All ideas";
            inviteModal.pickedIdeas = ({});
            inviteModal.expiry = "48 hours";
            inviteModal.uses = "Once";
            inviteModal.fingerprintChecked = false;
            inviteModal.confirmDiscard = false;
            inviteModal.reviewMode = false;
            inviteModal.reviewInvite = null;
            idField.text = "";
            noteField.text = "";
        }
        function openFresh() { inviteModal.reset(); inviteModal.open(); }
        function openReview(inv) {
            inviteModal.reset();
            inviteModal.reviewMode = true;
            inviteModal.reviewInvite = inv;
            inviteModal.method = "code";
            inviteModal.role = (inv && inv.role) || inviteModal.role;
            inviteModal.step = 3;
            inviteModal.open();
        }
        function hasInput() {
            return idField.text.trim().length > 0 || noteField.text.trim().length > 0;
        }
        // docs/03: Esc and Close dismiss, confirming only if step >= 2 has unsaved input.
        function requestClose() {
            if (inviteModal.step >= 2 && inviteModal.step <= 3 && !inviteModal.reviewMode
                    && inviteModal.hasInput()) {
                inviteModal.confirmDiscard = true;
                return;
            }
            inviteModal.close();
        }
        function advance() {
            if (inviteModal.step < 3) { inviteModal.step = inviteModal.step + 1; return; }
            if (inviteModal.step === 3) {
                if (inviteModal.reviewMode) {
                    // Approving a redeemed code seals the project key to the requester.
                    // TODO(phase1c): invites.approve(inviteId) — the requester's full OLB
                    // ID lives in core, so only what the invite row carries is passed on.
                    var inv = inviteModal.reviewInvite || ({});
                    screen.inviteRequested(inv.olbId || "", inviteModal.role);
                } else if (inviteModal.isCode) {
                    // TODO(phase1c): invites.createCode(role, ideaIds, expiry, uses) — a
                    // usable code has to be registered in core, so nothing is emitted here.
                } else {
                    screen.inviteRequested(idField.text.trim(), inviteModal.role);
                }
                inviteModal.step = 4;
                return;
            }
            inviteModal.close();
        }

        width: Theme.modalWidth
        height: Math.min(880, screen.height - Theme.s48)
        x: Math.round((screen.width - width) / 2)
        y: Math.max(Theme.s24, Math.round((screen.height - height) / 2))
        modal: true                              // modal popups trap focus
        padding: 0
        closePolicy: Popup.NoAutoClose           // the backdrop must not dismiss it

        Overlay.modal: Rectangle { color: Theme.backdrop }

        background: Item {
            Rectangle {                          // soft shadow
                x: 2; y: 8
                width: parent.width - 4
                height: parent.height
                radius: Theme.rModal
                color: Theme.modalShadow
            }
            Rectangle {
                anchors.fill: parent
                radius: Theme.rModal
                color: Theme.surface
                border.width: 1
                border.color: Theme.line
            }
        }

        onOpened: {
            if (inviteModal.step === 1 && !inviteModal.isCode) idField.forceActiveFocus();
            else primaryButton.forceActiveFocus();
        }
        onClosed: {
            inviteModal.reset();
            inviteButton.forceActiveFocus();     // focus returns to "Invite member"
        }

        // The modal body and the discard confirmation share one Item so the
        // confirmation can cover the body without the layout managing it.
        contentItem: Item {

            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                Shortcut {
                    sequence: "Esc"
                    enabled: inviteModal.opened
                    onActivated: inviteModal.confirmDiscard ? (inviteModal.confirmDiscard = false)
                                                            : inviteModal.requestClose()
                }

                // ---- header ----
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.margins: Theme.s20
                    spacing: Theme.s12

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s12

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s2
                            Text {
                                text: screen.projectName
                                textFormat: Text.PlainText
                                font.family: Theme.sans
                                font.pixelSize: Theme.overline
                                font.weight: Theme.medium
                                font.letterSpacing: Theme.overlineSpacing
                                color: Theme.ink3
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                            Text {
                                text: "Invite a member"
                                font.family: Theme.sans
                                font.pixelSize: Theme.modalTitle
                                font.weight: Theme.semibold
                                color: Theme.ink
                            }
                        }
                        Button {
                            text: "Close"
                            implicitHeight: Theme.touchTarget
                            Accessible.name: "Close the invite dialog"
                            onClicked: inviteModal.requestClose()
                        }
                    }

                    // ---- three-step bar ----
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s8

                        Repeater {
                            model: inviteModal.stepLabels
                            delegate: ColumnLayout {
                                required property string modelData
                                required property int index
                                readonly property int n: index + 1
                                readonly property bool done: n < inviteModal.step
                                readonly property bool current: n === inviteModal.step
                                Layout.fillWidth: true
                                spacing: Theme.s6

                                Rectangle {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: Theme.stepBar
                                    radius: Theme.rProgress
                                    color: done ? Theme.accent : (current ? Theme.violet : Theme.line)
                                }
                                Text {
                                    text: modelData
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.meta
                                    font.weight: current ? Theme.semibold : Theme.medium
                                    color: current ? Theme.ink : (done ? Theme.accent : Theme.ink3)
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                    Accessible.role: Accessible.StaticText
                                    Accessible.name: "Step " + n + ", " + modelData
                                                 + (done ? ", done" : (current ? ", current" : ", not started"))
                                }
                            }
                        }
                    }
                }

                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.line }

                // ---- body ----
                ScrollView {
                    id: modalBody
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: modalBody.availableWidth
                        spacing: Theme.s16

                        // =============================================== STEP 1
                        ColumnLayout {
                            visible: inviteModal.step === 1
                            Layout.fillWidth: true
                            Layout.margins: Theme.s20
                            spacing: Theme.s12

                            FieldLabel { text: "How do you want to invite them?" }

                            ChoiceCard {
                                Layout.fillWidth: true
                                radio: true
                                checked: !inviteModal.isCode
                                title: "Paste their Open Labbook ID"
                                body: "For someone who already uses Open Labbook. You check their key "
                                      + "fingerprint, then send."
                                onToggled: inviteModal.method = "id"
                            }
                            ChoiceCard {
                                Layout.fillWidth: true
                                radio: true
                                checked: inviteModal.isCode
                                title: "Share a one-time invite code"
                                body: "For someone new. They paste the code, their node replies with its "
                                      + "key, you approve."
                                onToggled: inviteModal.method = "code"
                            }

                            // ---------------- by Open Labbook ID ----------------
                            ColumnLayout {
                                visible: !inviteModal.isCode
                                Layout.fillWidth: true
                                spacing: Theme.s8

                                FieldLabel { text: "Their Open Labbook ID" }
                                TextField {
                                    id: idField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    placeholderText: screen.inviteDemo.olbId || "olb1:…"
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.body
                                    color: Theme.violet
                                    Accessible.name: "Their Open Labbook ID"
                                }
                                Text {
                                    text: "They copy it from Open Labbook, Profile, Copy my ID. It carries "
                                          + "their long-term sealing key, so it keeps working after they "
                                          + "restart Basecamp."
                                    font.family: Theme.sans; font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                                Text {
                                    visible: idField.text.trim().length > 0 && !inviteModal.idValid
                                    text: "This is not an Open Labbook ID. Ask them to copy it again."
                                    font.family: Theme.sans; font.pixelSize: Theme.meta
                                    font.weight: Theme.medium
                                    color: Theme.amber
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }

                                // ---- preview of the parsed ID ----
                                Rectangle {
                                    visible: inviteModal.idValid
                                    Layout.fillWidth: true
                                    radius: Theme.rInner
                                    color: Theme.surfaceSunken
                                    border.width: 1
                                    border.color: Theme.line
                                    implicitHeight: previewCol.implicitHeight + Theme.s16 * 2

                                    ColumnLayout {
                                        id: previewCol
                                        anchors.fill: parent
                                        anchors.margins: Theme.s16
                                        spacing: Theme.s8

                                        Text {
                                            text: inviteModal.parsed.name || ""
                                            textFormat: Text.PlainText
                                            font.family: Theme.sans; font.pixelSize: Theme.body
                                            font.weight: Theme.semibold
                                            color: Theme.ink
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        Text {
                                            text: "Name and ORCID as they set them · not verified by Open Labbook"
                                            font.family: Theme.sans; font.pixelSize: Theme.meta
                                            color: Theme.amber
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        RowLayout {
                                            Layout.fillWidth: true
                                            spacing: Theme.s6
                                            Rectangle {
                                                width: Theme.statusDot; height: Theme.statusDot
                                                radius: Theme.statusDot / 2
                                                color: inviteModal.parsed.online ? Theme.accent : Theme.dashed
                                            }
                                            Text {
                                                text: inviteModal.parsed.online
                                                      ? "Online now"
                                                      : "Offline · will deliver when they return"
                                                font.family: Theme.sans; font.pixelSize: Theme.small
                                                color: inviteModal.parsed.online ? Theme.accent : Theme.ink3
                                                wrapMode: Text.Wrap
                                                Layout.fillWidth: true
                                            }
                                        }

                                        FieldLabel { text: "Key fingerprint" }
                                        Rectangle {
                                            Layout.fillWidth: true
                                            radius: Theme.rControl
                                            color: Theme.surface
                                            border.width: 1
                                            border.color: Theme.line
                                            implicitHeight: fpText.implicitHeight + Theme.s12 * 2
                                            Text {
                                                id: fpText
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.verticalCenter: parent.verticalCenter
                                                anchors.margins: Theme.s12
                                                text: inviteModal.parsed.fingerprint || ""
                                                font.family: Theme.mono
                                                font.pixelSize: Theme.fingerprint
                                                color: Theme.violet
                                                wrapMode: Text.Wrap
                                                Accessible.role: Accessible.StaticText
                                                Accessible.name: "Key fingerprint " + (inviteModal.parsed.fingerprint || "")
                                            }
                                        }

                                        CheckBox {
                                            id: fpCheck
                                            Layout.fillWidth: true
                                            implicitHeight: Theme.touchTarget
                                            checked: inviteModal.fingerprintChecked
                                            text: "I compared this fingerprint with "
                                                  + screen.firstName(inviteModal.parsed.name)
                                                  + " in person or on a call"
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.small
                                            Accessible.name: text
                                            onToggled: inviteModal.fingerprintChecked = checked
                                        }
                                    }
                                }
                            }

                            // ---------------- by one-time code ----------------
                            ColumnLayout {
                                visible: inviteModal.isCode
                                Layout.fillWidth: true
                                spacing: Theme.s12

                                FieldLabel { text: "One-time invite code" }
                                Rectangle {
                                    Layout.fillWidth: true
                                    radius: Theme.rInner
                                    color: Theme.surfaceSunken
                                    border.width: 1
                                    border.color: Theme.line
                                    implicitHeight: codeText.implicitHeight + Theme.s16 * 2
                                    Text {
                                        id: codeText
                                        anchors.centerIn: parent
                                        text: inviteModal.code
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.code
                                        font.weight: Theme.medium
                                        color: Theme.violet
                                        Accessible.role: Accessible.StaticText
                                        Accessible.name: "One-time invite code " + inviteModal.code
                                    }
                                }
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s8
                                    Button {
                                        text: "Copy code"
                                        implicitHeight: Theme.touchTarget
                                        Accessible.name: "Copy code"
                                        onClicked: screen.copyToClipboard(inviteModal.code)
                                    }
                                    Button {
                                        text: "Make a new code"
                                        implicitHeight: Theme.touchTarget
                                        Accessible.name: "Make a new code"
                                        // TODO(phase1c): invites.newCode(expiry, uses) — generating a code
                                        // client-side would be a lie, the code hash has to be registered
                                        // in core before any node can redeem it (docs/04 §5).
                                        onClicked: {}
                                    }
                                    Item { Layout.fillWidth: true }
                                }

                                FieldLabel { text: "Expires after" }
                                PillRow {
                                    Layout.fillWidth: true
                                    groupName: "Expires after"
                                    entries: ["24 hours", "48 hours", "7 days"]
                                    selected: inviteModal.expiry
                                    onPicked: function (label) { inviteModal.expiry = label; }
                                }

                                FieldLabel { text: "Can be used" }
                                PillRow {
                                    Layout.fillWidth: true
                                    groupName: "Can be used"
                                    entries: ["Once", "Up to 5 times"]
                                    selected: inviteModal.uses
                                    onPicked: function (label) { inviteModal.uses = label; }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s6
                                    Repeater {
                                        model: ["Send the code through any channel you trust.",
                                                "They open Open Labbook, Join a project, and paste it.",
                                                "Their node sends back its key. The request appears in "
                                                + "Members for you to approve."]
                                        delegate: RowLayout {
                                            required property string modelData
                                            required property int index
                                            Layout.fillWidth: true
                                            spacing: Theme.s8
                                            Text {
                                                text: (index + 1) + "."
                                                font.family: Theme.mono; font.pixelSize: Theme.small
                                                color: Theme.ink3
                                                Layout.alignment: Qt.AlignTop
                                            }
                                            Text {
                                                text: modelData
                                                font.family: Theme.sans; font.pixelSize: Theme.small
                                                color: Theme.ink2
                                                wrapMode: Text.Wrap
                                                Layout.fillWidth: true
                                            }
                                        }
                                    }
                                }
                                Text {
                                    text: "The code holds no project data. It only lets their node find yours."
                                    font.family: Theme.sans; font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        // =============================================== STEP 2
                        ColumnLayout {
                            visible: inviteModal.step === 2
                            Layout.fillWidth: true
                            Layout.margins: Theme.s20
                            spacing: Theme.s12

                            FieldLabel { text: "Role" }
                            Repeater {
                                model: [
                                    { label: "Contributor",
                                      desc: "Writes entries, opens branches and approves merges into main." },
                                    { label: "Reviewer",
                                      desc: "Reads the ideas you choose and approves funding milestones. Cannot edit records." },
                                    { label: "Lead",
                                      desc: "Everything a contributor can do, plus publishing and inviting others." }
                                ]
                                delegate: ChoiceCard {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    radio: true
                                    checked: inviteModal.role === modelData.label
                                    title: modelData.label
                                    body: modelData.desc
                                    onToggled: inviteModal.role = modelData.label
                                }
                            }

                            FieldLabel { text: "Which ideas can they see?" }
                            PillRow {
                                Layout.fillWidth: true
                                groupName: "Which ideas can they see?"
                                entries: ["All ideas", "Selected ideas"]
                                selected: inviteModal.access
                                onPicked: function (label) { inviteModal.access = label; }
                            }

                            ColumnLayout {
                                visible: inviteModal.access === "Selected ideas"
                                Layout.fillWidth: true
                                spacing: Theme.s2
                                Repeater {
                                    model: screen.accessIdeas
                                    delegate: Rectangle {
                                        required property var modelData
                                        readonly property string ideaId: modelData.id || modelData.title || ""
                                        Layout.fillWidth: true
                                        Layout.preferredHeight: Theme.touchTarget
                                        color: "transparent"
                                        CheckBox {
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.verticalCenter: parent.verticalCenter
                                            checked: !!inviteModal.pickedIdeas[parent.ideaId]
                                            text: modelData.title || modelData.id || ""
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.small
                                            Accessible.name: text
                                            onToggled: {
                                                var next = {};
                                                for (var k in inviteModal.pickedIdeas)
                                                    next[k] = inviteModal.pickedIdeas[k];
                                                next[parent.ideaId] = checked;
                                                inviteModal.pickedIdeas = next;
                                            }
                                        }
                                    }
                                }
                            }

                            Text {
                                text: "Each idea's records are sealed separately, so access can be widened later."
                                font.family: Theme.sans; font.pixelSize: Theme.meta
                                color: Theme.ink3
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }

                            FieldLabel { text: "Note to them (optional)" }
                            TextArea {
                                id: noteField
                                Layout.fillWidth: true
                                Layout.preferredHeight: 88
                                font.family: Theme.sans
                                font.pixelSize: Theme.body
                                Accessible.name: "Note to them, optional"
                            }
                            Text {
                                text: "Sent end-to-end encrypted with the invite."
                                font.family: Theme.sans; font.pixelSize: Theme.meta
                                color: Theme.ink3
                                Layout.fillWidth: true
                            }
                        }

                        // =============================================== STEP 3
                        ColumnLayout {
                            visible: inviteModal.step === 3
                            Layout.fillWidth: true
                            Layout.margins: Theme.s20
                            spacing: Theme.s16

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s8
                                Repeater {
                                    model: inviteModal.summaryRows
                                    delegate: RowLayout {
                                        required property var modelData
                                        readonly property bool isKey: modelData.k === "Key fingerprint"
                                                                      || String(modelData.v).indexOf("Code ") === 0
                                        Layout.fillWidth: true
                                        spacing: Theme.s12
                                        Text {
                                            text: modelData.k
                                            Layout.preferredWidth: 140
                                            font.family: Theme.sans; font.pixelSize: Theme.small
                                            color: Theme.ink3
                                        }
                                        Text {
                                            text: modelData.v
                                            textFormat: Text.PlainText
                                            font.family: isKey ? Theme.mono : Theme.sans
                                            font.pixelSize: Theme.small
                                            font.weight: Theme.medium
                                            color: isKey ? Theme.violet : Theme.ink
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                    }
                                }
                            }

                            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.lineSoft }

                            FieldLabel { text: "What happens when you send" }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s6
                                Repeater {
                                    model: inviteModal.happensRows
                                    delegate: RowLayout {
                                        required property string modelData
                                        required property int index
                                        Layout.fillWidth: true
                                        spacing: Theme.s8
                                        Text {
                                            text: (index + 1) + "."
                                            font.family: Theme.mono; font.pixelSize: Theme.small
                                            color: Theme.ink3
                                            Layout.alignment: Qt.AlignTop
                                        }
                                        Text {
                                            text: modelData
                                            textFormat: Text.PlainText
                                            font.family: Theme.sans; font.pixelSize: Theme.small
                                            color: Theme.ink2
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                    }
                                }
                            }

                            // Amber testnet notice. docs/03 M1 step 3 replaces the design's
                            // Chat-address wording: a Chat address is never an identity, and the
                            // envelope travels over a Delivery inbox topic (docs/04 §3).
                            Rectangle {
                                visible: !inviteModal.isCode || inviteModal.reviewMode
                                Layout.fillWidth: true
                                radius: Theme.rInner
                                color: Theme.amberTint
                                implicitHeight: noticeText.implicitHeight + Theme.s12 * 2
                                Text {
                                    id: noticeText
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.margins: Theme.s12
                                    text: "Testnet: if they are offline for long, the network may drop the "
                                          + "message. Resend from Members, Pending invites — nothing else "
                                          + "needs redoing."
                                    font.family: Theme.sans; font.pixelSize: Theme.meta
                                    color: Theme.amber
                                    wrapMode: Text.Wrap
                                }
                            }
                        }

                        // =============================================== STEP 4
                        ColumnLayout {
                            visible: inviteModal.step === 4
                            Layout.fillWidth: true
                            Layout.margins: Theme.s20
                            spacing: Theme.s16

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                Text {
                                    text: inviteModal.doneTitle
                                    textFormat: Text.PlainText
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.cardTitle
                                    font.weight: Theme.semibold
                                    color: Theme.ink
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: inviteModal.doneSub
                                    textFormat: Text.PlainText
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.small
                                    color: Theme.ink3
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s12
                                Repeater {
                                    model: inviteModal.progressRows
                                    delegate: RowLayout {
                                        required property var modelData
                                        readonly property bool done: modelData.s === "done"
                                        readonly property bool now: modelData.s === "now"
                                        Layout.fillWidth: true
                                        spacing: Theme.s12

                                        Rectangle {
                                            Layout.alignment: Qt.AlignVCenter
                                            width: 14; height: 14; radius: 7
                                            color: done ? Theme.accent : Theme.surface
                                            border.width: 2
                                            border.color: done ? Theme.accent
                                                               : (now ? Theme.violet : Theme.inputBorder)
                                        }
                                        Text {
                                            text: modelData.l
                                            textFormat: Text.PlainText
                                            font.family: Theme.sans; font.pixelSize: Theme.small
                                            color: (done || now) ? Theme.ink : Theme.ink3
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        Text {
                                            text: done ? "Done" : (now ? "Waiting" : "Not yet")
                                            font.family: Theme.sans; font.pixelSize: Theme.meta
                                            font.weight: Theme.medium
                                            color: done ? Theme.accent : (now ? Theme.violet : Theme.ink3)
                                        }
                                    }
                                }
                            }

                            Text {
                                text: "You can follow this in Members, Pending invites. A signed commit "
                                      + "records the change in History once they accept."
                                font.family: Theme.sans; font.pixelSize: Theme.meta
                                color: Theme.ink3
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }
                        }
                    }
                }

                Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.line }

                // ---- footer ----
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Theme.s20
                    spacing: Theme.s8

                    Text {
                        text: inviteModal.step < 4 ? ("Step " + inviteModal.step + " of 3") : "Invite pending"
                        font.family: Theme.sans; font.pixelSize: Theme.meta
                        color: Theme.ink3
                        Layout.fillWidth: true
                    }
                    Button {
                        visible: inviteModal.step === 2 || (inviteModal.step === 3 && !inviteModal.reviewMode)
                        text: "Back"
                        implicitHeight: Theme.touchTarget
                        Accessible.name: "Back"
                        onClicked: inviteModal.step = inviteModal.step - 1
                    }
                    Button {
                        visible: inviteModal.step === 4
                        text: "Invite another"
                        implicitHeight: Theme.touchTarget
                        Accessible.name: "Invite another member"
                        onClicked: { inviteModal.reset(); idField.forceActiveFocus(); }
                    }
                    PrimaryButton {
                        id: primaryButton
                        text: inviteModal.primaryLabel
                        implicitHeight: Theme.touchTarget
                        enabled: inviteModal.canContinue
                        Accessible.name: inviteModal.primaryLabel
                        onClicked: inviteModal.advance()
                    }
                }
            }

            // ---- discard confirmation, drawn inside the modal so focus stays trapped ----
            // docs/03 asks for this confirmation but does not write its copy, and it is
            // not in the design files; the wording below is the minimum that is true.
            Rectangle {
                anchors.fill: parent
                visible: inviteModal.confirmDiscard
                color: Theme.backdrop
                radius: Theme.rModal

                MouseArea { anchors.fill: parent }    // swallow clicks behind the card

                Rectangle {
                    anchors.centerIn: parent
                    width: Math.min(400, parent.width - Theme.s32 * 2)
                    height: discardCol.implicitHeight + Theme.s20 * 2
                    radius: Theme.rPanel
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.line

                    ColumnLayout {
                        id: discardCol
                        anchors.fill: parent
                        anchors.margins: Theme.s20
                        spacing: Theme.s12

                        Text {
                            text: "Discard this invite?"
                            font.family: Theme.sans
                            font.pixelSize: Theme.sectionTitle
                            font.weight: Theme.semibold
                            color: Theme.ink
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                        Text {
                            text: "Nothing has been sent yet."
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s8
                            Item { Layout.fillWidth: true }
                            Button {
                                text: "Keep editing"
                                implicitHeight: Theme.touchTarget
                                Accessible.name: "Keep editing"
                                onClicked: inviteModal.confirmDiscard = false
                            }
                            PrimaryButton {
                                text: "Discard"
                                implicitHeight: Theme.touchTarget
                                Accessible.name: "Discard this invite"
                                onClicked: { inviteModal.confirmDiscard = false; inviteModal.close(); }
                            }
                        }
                    }
                }
            }
        }
    }
}
