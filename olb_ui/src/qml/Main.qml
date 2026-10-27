import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Shell: left nav + the active screen. docs/03 names the nav entries; the
// ones not implemented yet stay visibly disabled rather than pretending to
// work.
Rectangle {
    id: root
    color: Theme.ground

    readonly property var backend: logos.module("olb_ui")
    property bool ready: false
    property string screen: "Public Board"

    readonly property var boardData: {
        try { return JSON.parse(backend ? backend.board : "{}"); } catch (e) { return ({}); }
    }
    readonly property bool fixtures: backend ? backend.fixtures : false

    function parseJson(text, fallback) {
        try { return JSON.parse(text); } catch (e) { return fallback; }
    }
    readonly property var notesModel: parseJson(backend ? backend.notes : "[]", [])
    readonly property var projectsModel: parseJson(backend ? backend.projects : "[]", [])
    readonly property var profileObj: parseJson(backend ? backend.profile : "{}", ({}))
    readonly property var boardProjects: root.boardData.projects || []
    readonly property var sessionObj: parseJson(backend ? backend.coreStatus : "{}", ({}))
    readonly property var historyObj: parseJson(backend ? backend.history : "{}", ({}))
    readonly property var membersObj: parseJson(backend ? backend.members : "{}", ({}))
    // {id, fingerprint} — the olb1: string and the only part a human checks.
    // The store is sealed until a passphrase opens it, so this gates the
    // whole shell rather than appearing as one more screen in the nav.
    readonly property var lockObj: parseJson(backend ? backend.lockState : "{}", ({}))
    readonly property bool unlocked: (lockObj.state || "") === "unlocked"
    readonly property var myIdObj: parseJson(backend ? backend.myId : "{}", ({}))
    // Invites waiting on a decision. Never auto-accepted (docs/04 §9).
    readonly property var invitesArr: parseJson(backend ? backend.invites : "[]", [])
    // The project an invite would be sent into.
    readonly property string currentProjectId: {
        const b = parseJson(backend ? backend.board : "{}", ({}));
        return b.projectId || "";
    }
    readonly property var openedRecord: parseJson(backend ? backend.openedRecord : "{}", ({}))
    readonly property var inviteDemoObj: ({})
    property string historyFilterIdea: ""

    // docs/03 nav. `done` marks what actually exists today.
    readonly property var navItems: [
        { label: "Public Board",      done: true  },
        { label: "Personal Notebook", done: true  },
        { label: "Notebook entry",    done: true  },
        { label: "New project",       done: true  },
        { label: "History",           done: true  },
        { label: "Members",           done: true  },
        { label: "Hypotheses",        done: true  },
        { label: "Funding",           done: true  },
        { label: "Publish",           done: true  },
        { label: "Settings",          done: true  },
        { label: "Diagnostics",       done: true  }
    ]
    function screenIndex(name) {
        switch (name) {
        case "Public Board":      return 0;
        case "Personal Notebook": return 1;
        case "New project":       return 2;
        case "Settings":          return 3;
        case "Notebook entry":    return 4;
        case "History":           return 5;
        case "Members":           return 6;
        case "Hypotheses":        return 7;
        case "Funding":           return 8;
        case "Publish":           return 9;
        default:                  return 10;  // Diagnostics
        }
    }

    // Opening a record loads it, then shows S2.
    function showRecord(recordId) {
        if (!root.backend || recordId === "")
            return;
        root.backend.openRecord(recordId);
        root.screen = "Notebook entry";
    }

    Connections {
        target: logos
        function onViewModuleReadyChanged(moduleName, isReady) {
            if (moduleName === "olb_ui") {
                root.ready = isReady && root.backend !== null;
                if (root.ready) root.backend.refreshBoard();
            }
        }
    }
    Component.onCompleted: {
        root.ready = root.backend !== null && logos.isViewModuleReady("olb_ui");
        if (root.ready) {
            root.backend.refreshLockState();
            root.backend.refreshBoard();
        }
    }

    // The passphrase gate. Until the store opens there is nothing behind
    // this to show, so it replaces the shell rather than overlaying it —
    // a greyed-out nav would imply the data is there and merely disabled.
    Unlock {
        anchors.fill: parent
        visible: !root.unlocked
        lockState: root.lockObj
        lastResult: root.backend ? root.backend.lastActionResult : ""
        onCreatePassphrase: function (p) { if (root.backend) root.backend.createPassphrase(p); }
        onUnlockStore: function (p) { if (root.backend) root.backend.unlockStore(p); }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0
        visible: root.unlocked

        // ---- Left nav ----
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 200
            color: Theme.surface
            border.width: 0

            Rectangle {   // right hairline
                anchors.right: parent.right
                width: 1; height: parent.height
                color: Theme.line
            }

            ColumnLayout {
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.margins: Theme.s12
                spacing: Theme.s2

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Theme.s8
                    spacing: Theme.s8
                    Image {
                        source: "../assets/icon.png"
                        sourceSize.width: 24
                        sourceSize.height: 24
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 24
                        fillMode: Image.PreserveAspectFit
                        Accessible.role: Accessible.Graphic
                        Accessible.name: "Open Labbook"
                    }
                    Text {
                        text: "Open Labbook"
                        font.family: Theme.sans
                        font.pixelSize: Theme.small
                        font.weight: Theme.semibold
                        color: Theme.ink
                        Layout.fillWidth: true
                    }
                }

                Text {
                    text: "PROJECTS"
                    font.family: Theme.sans
                    font.pixelSize: Theme.overline
                    font.weight: Theme.medium
                    font.letterSpacing: Theme.overlineSpacing
                    color: Theme.ink3
                    Layout.margins: Theme.s8
                    Layout.topMargin: Theme.s12
                    visible: root.boardProjects.length > 0
                }

                Repeater {
                    model: root.boardProjects
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        radius: Theme.rControl
                        color: modelData.current ? Theme.accentTint : "transparent"

                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.title
                        activeFocusOnTab: true

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.leftMargin: Theme.s12
                            anchors.rightMargin: Theme.s8
                            text: modelData.title || "Untitled"
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            font.weight: modelData.current ? Theme.medium : Theme.regular
                            color: modelData.current ? Theme.accent : Theme.ink2
                            elide: Text.ElideRight
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                root.backend.setCurrentProject(modelData.id);
                                root.screen = "Public Board";
                            }
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.s12
                    height: 1
                    color: Theme.lineSoft
                    visible: root.boardProjects.length > 0
                }

                Repeater {
                    model: root.navItems
                    delegate: Rectangle {
                        required property var modelData
                        readonly property bool active: root.screen === modelData.label
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        radius: Theme.rControl
                        color: active ? Theme.accentTint : "transparent"

                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.label
                                         + (modelData.done ? "" : ", not built yet")
                        activeFocusOnTab: modelData.done

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.left: parent.left
                            anchors.leftMargin: Theme.s12
                            text: modelData.label
                            font.family: Theme.sans
                            font.pixelSize: Theme.body
                            font.weight: parent.active ? Theme.medium : Theme.regular
                            color: !modelData.done ? Theme.dashed
                                                   : (parent.active ? Theme.accent : Theme.ink2)
                        }

                        MouseArea {
                            anchors.fill: parent
                            enabled: modelData.done
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: root.screen = modelData.label
                        }
                        Keys.onReturnPressed: if (modelData.done) root.screen = modelData.label
                    }
                }
            }
        }

        // ---- Active screen ----
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.screenIndex(root.screen)

            Board {
                boardData: root.boardData
                fixtures: root.fixtures
                onOpenRecord: function (recordId) { root.showRecord(recordId); }
                onOpenHistory: function (ideaId) {
                    root.historyFilterIdea = ideaId;
                    root.screen = "History";
                }
                onNewProject: root.screen = "New project"
                onNewEntry: root.screen = "Personal Notebook"
                onRefresh: if (root.backend) root.backend.refreshBoard()
            }

            Notebook {
                backend: root.backend
                notes: root.notesModel
                projects: root.projectsModel
            }

            NewProject {
                backend: root.backend
                // Only reached once the core has confirmed, so the board we
                // land on already has the project on it.
                onCreated: root.screen = "Public Board"
                onCancelled: root.screen = "Public Board"
            }

            Settings {
                backend: root.backend
                profile: root.profileObj
                coreStatus: root.backend ? root.backend.coreStatus : ""
                myId: root.myIdObj
                invites: root.invitesArr
                onAcceptInvite: function (inviteId) {
                    if (root.backend) root.backend.acceptInvite(inviteId);
                }
            }

            Entry {
                backend: root.backend
                record: root.openedRecord
                onOpenHistory: function (recordId) {
                    root.historyFilterIdea = recordId;
                    root.screen = "History";
                }
                onOpenFile: function (cid) { console.log("[olb_ui] open file:", cid); }
            }

            History {
                backend: root.backend
                history: root.historyObj
                filterIdeaId: root.historyFilterIdea
                onOpenCommit: function (commitId) { console.log("[olb_ui] open commit:", commitId); }
                onOpenRecord: function (recordId) { root.showRecord(recordId); }
            }

            Members {
                backend: root.backend
                members: root.membersObj
                inviteDemo: root.inviteDemoObj
                onInviteRequested: function (olbId, role) {
                    if (!root.backend) return;
                    if (!root.currentProjectId) {
                        console.log("[olb_ui] no project selected to invite into");
                        return;
                    }
                    root.backend.inviteMember(root.currentProjectId, olbId, role);
                }
                onMemberRemoved: function (memberId) {
                    console.log("[olb_ui] member removed:", memberId);
                }
                onKeyRotationRequested: {
                    console.log("[olb_ui] key rotation requested");
                }
            }

            // S3, S4 and S5 render the fixture sections (Fixtures.qml, generated
            // from fixtures/sample-project.json). The .rep carries no
            // hypotheses, funding or publish PROP yet; when it does, these
            // three bindings move to root.parseJson(backend.<prop>, ...) and
            // the screens stay as they are.
            Hypotheses {
                hypothesesData: Fixtures.hypotheses.h1
                onOpenBoard: root.screen = "Public Board"
                onOpenRecord: function (recordId) { root.showRecord(recordId); }
                onCommitRequested: function (hypothesisId, sealedCopyForReviewers) {
                    // TODO(phase2): hypothesis.commit(hypothesisId, options) —
                    // needs the olb_registry LEZ program (docs/07).
                    console.log("[olb_ui] commit hypothesis:", hypothesisId,
                                "sealed copy:", sealedCopyForReviewers);
                }
                onDraftSaveRequested: function (hypothesisId, plan) {
                    // TODO(phase2): hypothesis.saveDraft(hypothesisId, planJson).
                    console.log("[olb_ui] save hypothesis draft:", hypothesisId);
                }
                onPreviewRequested: function (hypothesisId, plan) {
                    // TODO(phase2): hypothesis.previewCommitment(hypothesisId, planJson).
                    console.log("[olb_ui] preview commitment:", hypothesisId);
                }
            }

            Funding {
                fundingData: Fixtures.funding["prop-lid"]
                onOpenBoard: root.screen = "Public Board"
                onOpenEvidence: function (milestoneId, recordId, label) {
                    if (recordId !== "") root.showRecord(recordId);
                    else console.log("[olb_ui] evidence has no record yet:", label);
                }
                onApproveRelease: function (milestoneId) {
                    // TODO(phase3): funding.approve(milestoneId) — the release
                    // happens on-chain in olb_funding, never locally.
                    console.log("[olb_ui] approve release:", milestoneId);
                }
                onEvidenceRequested: function (milestoneId, note) {
                    // TODO(phase3): funding.requestEvidence(milestoneId, note);
                    // the note needs a prompt the canvas does not draw yet.
                    console.log("[olb_ui] ask for more evidence:", milestoneId);
                }
                onContributeRequested: {
                    // TODO(phase3): funding.contribute(amount, private).
                    console.log("[olb_ui] contribute requested");
                }
                onOpenProposal: console.log("[olb_ui] view proposal")
            }

            Publish {
                publishData: Fixtures.publish
                onOpenBoard: root.screen = "Public Board"
                onPublishRequested: function (options) {
                    // TODO(phase4): publish.execute(options).
                    console.log("[olb_ui] publish bundle:", JSON.stringify(options));
                }
                onPinRequested: function (revealId, cid) {
                    // TODO(phase1b): files.pin(cid) — the .rep has no pin SLOT.
                    console.log("[olb_ui] pin to replica:", revealId, cid);
                }
            }

            Diagnostics {
                backend: root.backend
                ready: root.ready
            }
        }
    }
}
