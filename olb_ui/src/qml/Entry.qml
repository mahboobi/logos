import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S2 · Notebook entry (docs/03, design/canvas/Entry.dc.html).
//
// One experiment record: write it, review agent suggestions, see provenance
// and versions. Everything is rendered from `record` — the shape of
// fixtures/sample-project.json records["entry-run14"]. Actions that need core
// methods the .rep does not expose yet are marked TODO(phase1b) and only log;
// nothing here invents an olb_core call.
Item {
    id: entry

    // ---- Interface contract ----
    property var backend: null          // may be null; guard every use
    property var record: ({})           // the entry record
    signal openHistory(string recordId)
    signal openFile(string cid)

    // ---- Derived, all guarded: `record` can be empty ----
    readonly property string recordId: entry.record.id || ""
    readonly property string title: entry.record.title || ""
    // Breadcrumb leaf is the short name ahead of the title's separator.
    readonly property string shortTitle: entry.title.indexOf(" · ") > 0
                                         ? entry.title.substring(0, entry.title.indexOf(" · "))
                                         : entry.title
    // The record carries no stage of its own yet; Experiments is where entries
    // live (docs/03 S2 breadcrumb).
    readonly property string stageName: entry.record.stage || "Experiments"
    readonly property var follows: entry.record.follows || []
    readonly property var parameters: entry.record.parameters || []
    readonly property var files: entry.record.files || []
    readonly property var suggestions: entry.record.suggestions || []
    readonly property var agent: entry.record.agent || ({})
    readonly property var provenance: entry.record.provenance || ({})
    readonly property var signatures: entry.record.signatures || []
    readonly property var anchor: entry.record.anchor || ({})
    readonly property var versions: entry.record.versions || []

    readonly property bool twoColumn: entry.width > 1040
    // Parameter table column shares: Parameter / Value / Unit / Source.
    readonly property var paramCols: [0.30, 0.26, 0.14, 0.30]

    // Draft badge copy follows the commit lifecycle in docs/03 S2.
    function statusText() {
        if (entry.record.status === "shared")
            return "Shared · waiting for anchor";
        if (entry.record.status === "anchored")
            return entry.anchor.batch ? "Anchored in batch " + entry.anchor.batch
                                      : (entry.anchor.current || "");
        return "Draft · not yet shared with group";
    }
    function statusKind() {
        return entry.record.status === "anchored" ? "done" : "attention";
    }

    // ---- A tinted, keyboard-reachable button ----
    // Accept / Pin to replica need fills the stock Button style cannot carry
    // (accent tint, amber outline), and colour is meaning here, not decoration.
    component TintButton: Rectangle {
        id: tint
        property string label: ""
        property string accessibleName: ""
        property color fg: Theme.ink
        property color edge: Theme.inputBorder
        property int weight: Theme.medium
        signal clicked()

        implicitHeight: Theme.touchTarget
        implicitWidth: tintLabel.implicitWidth + Theme.s16 * 2
        radius: Theme.rControl
        border.width: 1
        border.color: tint.edge

        Accessible.role: Accessible.Button
        Accessible.name: tint.accessibleName !== "" ? tint.accessibleName : tint.label
        activeFocusOnTab: true

        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: Theme.rControl
            color: "transparent"
            border.width: 2
            border.color: Theme.violet
            visible: parent.activeFocus
        }

        Text {
            id: tintLabel
            anchors.centerIn: parent
            text: tint.label
            font.family: Theme.sans
            font.pixelSize: Theme.body
            font.weight: tint.weight
            color: tint.fg
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: tint.clicked()
        }
        Keys.onReturnPressed: tint.clicked()
        Keys.onSpacePressed: tint.clicked()
    }

    // ---- A text link that still hits the 44 px target ----
    component LinkRow: Item {
        id: link
        property string label: ""
        property color fg: Theme.accent
        signal clicked()

        implicitHeight: Theme.touchTarget
        implicitWidth: linkLabel.implicitWidth

        Accessible.role: Accessible.Link
        Accessible.name: link.label
        activeFocusOnTab: true

        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: Theme.rControl
            color: "transparent"
            border.width: 2
            border.color: Theme.violet
            visible: parent.activeFocus
        }
        Text {
            id: linkLabel
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            text: link.label
            font.family: Theme.sans
            font.pixelSize: Theme.body
            font.weight: Theme.medium
            color: link.fg
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: link.clicked()
        }
        Keys.onReturnPressed: link.clicked()
    }

    // Edits autosave as a local draft, debounced 800 ms (docs/03 S2).
    Timer {
        id: draftTimer
        interval: 800
        repeat: false
        // TODO(phase1b): needs record.saveDraft(recordId, fieldsJson) on the
        // olb_ui backend; until then the edit stays in the text fields only.
        onTriggered: console.log("[olb_ui] draft save pending for:", entry.recordId)
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: entry.width
            spacing: Theme.s20

            // ---- Breadcrumb ----
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.topMargin: Theme.s28
                spacing: Theme.s6

                // TODO(phase1b): the trail is flat text until the shell gives
                // this screen a navigation signal for Board and the stage view.
                Text {
                    text: "Board"
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
                    text: entry.stageName
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
                    text: entry.shortTitle
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink
                }
                Item { Layout.fillWidth: true }
            }

            // ---- Header: title, badges, actions ----
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                spacing: Theme.s8

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s16

                    Text {
                        text: entry.title
                        font.family: Theme.sans
                        font.pixelSize: Theme.pageTitle
                        font.weight: Theme.semibold
                        font.letterSpacing: Theme.titleSpacing
                        color: Theme.ink
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }

                    Button {
                        text: "Mark as negative result"
                        implicitHeight: Theme.touchTarget
                        Accessible.name: "Mark as negative result"
                        // TODO(phase1b): needs record.markNegative(recordId) —
                        // its own signed commit, so it cannot be local state.
                        onClicked: console.log("[olb_ui] mark as negative result:", entry.recordId)
                    }
                    PrimaryButton {
                        text: "Sign and share with group"
                        implicitHeight: Theme.touchTarget
                        Accessible.name: "Sign and share with group"
                        // TODO(phase1b): needs record.commit(recordId, message).
                        onClicked: console.log("[olb_ui] sign and share:", entry.recordId)
                    }
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: Theme.s6

                    Badge {
                        visible: !!entry.record.version
                        text: "Version " + (entry.record.version || "")
                        kind: "neutral"
                    }
                    Badge {
                        text: entry.statusText()
                        kind: entry.statusKind()
                        visible: entry.statusText() !== ""
                    }
                    Badge {
                        visible: (entry.record.tests || "") !== ""
                        text: "Tests " + (entry.record.tests || "")
                        kind: "current"
                    }
                    Badge {
                        visible: !!entry.record.negativeResult
                        text: "Negative result"
                        kind: "neutral"
                    }
                }
            }

            // ---- Two columns: record body, then agent / provenance / versions ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s48
                columns: entry.twoColumn ? 2 : 1
                columnSpacing: Theme.s20
                rowSpacing: Theme.s20

                // ================= MAIN COLUMN =================
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s16

                    // ---- Purpose and protocol ----
                    Panel {
                        title: "Purpose and protocol"
                        Layout.fillWidth: true

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s6

                            FieldLabel { text: "Purpose" }
                            TextArea {
                                id: purposeField
                                Layout.fillWidth: true
                                Layout.preferredHeight: 72
                                text: entry.record.purpose || ""
                                wrapMode: TextArea.Wrap
                                font.family: Theme.sans
                                font.pixelSize: Theme.body
                                color: Theme.ink
                                Accessible.name: "Purpose"
                                onTextChanged: draftTimer.restart()
                            }
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: Theme.s10

                            FieldLabel {
                                text: "Follows"
                                height: Theme.chipHeight
                                verticalAlignment: Text.AlignVCenter
                            }
                            // TODO(phase1b): following a chip needs an
                            // openRecord(recordId) signal the contract for this
                            // screen does not carry, so these are labels.
                            Repeater {
                                model: entry.follows
                                delegate: Rectangle {
                                    required property var modelData
                                    readonly property bool isCurrent: modelData.kind === "current"
                                    implicitHeight: Theme.chipHeight
                                    implicitWidth: chipText.implicitWidth + Theme.s12 * 2
                                    radius: Theme.rPill
                                    color: isCurrent ? Theme.violetTint : Theme.ground
                                    border.width: isCurrent ? 0 : 1
                                    border.color: Theme.line

                                    Accessible.role: Accessible.StaticText
                                    Accessible.name: "Follows " + (modelData.label || "")

                                    Text {
                                        id: chipText
                                        anchors.centerIn: parent
                                        text: modelData.label || ""
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.small
                                        color: parent.isCurrent ? Theme.violet : Theme.ink
                                    }
                                }
                            }
                        }
                    }

                    // ---- Parameters ----
                    Panel {
                        id: paramsPanel
                        title: "Parameters"
                        subtitle: "Source shows where each value came from"
                        Layout.fillWidth: true

                        readonly property real tableWidth: paramsPanel.width - Theme.s20 * 2
                        function colWidth(i) {
                            return Math.max(56, paramsPanel.tableWidth * entry.paramCols[i]);
                        }

                        // Header row
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                Repeater {
                                    model: ["Parameter", "Value", "Unit", "Source"]
                                    delegate: Text {
                                        required property var modelData
                                        required property int index
                                        Layout.preferredWidth: paramsPanel.colWidth(index)
                                        Layout.fillWidth: index === 3
                                        leftPadding: Theme.s10
                                        rightPadding: Theme.s10
                                        bottomPadding: Theme.s8
                                        topPadding: Theme.s8
                                        text: modelData
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        font.weight: Theme.medium
                                        color: Theme.ink3
                                    }
                                }
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                implicitHeight: 1
                                color: Theme.line
                            }

                            Repeater {
                                model: entry.parameters
                                delegate: ColumnLayout {
                                    required property var modelData
                                    required property int index
                                    Layout.fillWidth: true
                                    spacing: 0

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 0

                                        Text {
                                            Layout.preferredWidth: paramsPanel.colWidth(0)
                                            padding: Theme.s10
                                            text: modelData.name || ""
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.body
                                            color: Theme.ink
                                            wrapMode: Text.Wrap
                                        }
                                        Text {
                                            Layout.preferredWidth: paramsPanel.colWidth(1)
                                            padding: Theme.s10
                                            text: modelData.value || ""
                                            font.family: Theme.mono
                                            font.pixelSize: Theme.body
                                            color: Theme.ink
                                            wrapMode: Text.Wrap
                                        }
                                        Text {
                                            Layout.preferredWidth: paramsPanel.colWidth(2)
                                            padding: Theme.s10
                                            text: modelData.unit || ""
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.body
                                            color: Theme.ink
                                            wrapMode: Text.Wrap
                                        }
                                        Text {
                                            Layout.preferredWidth: paramsPanel.colWidth(3)
                                            Layout.fillWidth: true
                                            padding: Theme.s10
                                            text: modelData.source || ""
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.body
                                            color: Theme.ink2
                                            wrapMode: Text.Wrap
                                        }
                                    }
                                    Rectangle {
                                        Layout.fillWidth: true
                                        implicitHeight: 1
                                        color: Theme.lineSoft
                                        visible: index < entry.parameters.length - 1
                                    }
                                }
                            }
                        }

                        LinkRow {
                            label: "+ Add parameter"
                            Layout.alignment: Qt.AlignLeft
                            // TODO(phase1b): needs record.saveDraft with the new
                            // parameter row; nothing is added locally.
                            onClicked: console.log("[olb_ui] add parameter to:", entry.recordId)
                        }
                    }

                    // ---- Observations and deviations ----
                    Panel {
                        title: "Observations and deviations"
                        Layout.fillWidth: true

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s6
                            FieldLabel { text: "Observations" }
                            TextArea {
                                id: observationsField
                                Layout.fillWidth: true
                                Layout.preferredHeight: 96
                                text: entry.record.observations || ""
                                wrapMode: TextArea.Wrap
                                font.family: Theme.sans
                                font.pixelSize: Theme.body
                                color: Theme.ink
                                Accessible.name: "Observations"
                                onTextChanged: draftTimer.restart()
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s6
                            FieldLabel { text: "Deviations from protocol" }
                            TextArea {
                                id: deviationsField
                                Layout.fillWidth: true
                                Layout.preferredHeight: 72
                                text: entry.record.deviations || ""
                                wrapMode: TextArea.Wrap
                                font.family: Theme.sans
                                font.pixelSize: Theme.body
                                color: Theme.ink
                                Accessible.name: "Deviations from protocol"
                                onTextChanged: draftTimer.restart()
                            }
                        }
                    }

                    // ---- Files ----
                    Panel {
                        title: "Files"
                        Layout.fillWidth: true

                        RowLayout {
                            Layout.fillWidth: true
                            Item { Layout.fillWidth: true }
                            Button {
                                text: "Add files"
                                implicitHeight: Theme.touchTarget
                                Accessible.name: "Add files"
                                // TODO(phase1b): needs record.addFiles(recordId,
                                // absPaths) — the sandbox means the picker and
                                // the upload both live in the backend.
                                onClicked: console.log("[olb_ui] add files to:", entry.recordId)
                            }
                        }

                        Repeater {
                            model: entry.files
                            delegate: ColumnLayout {
                                required property var modelData
                                required property int index
                                readonly property bool single: (modelData.copies || 0) <= 1
                                Layout.fillWidth: true
                                spacing: 0

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s12

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        Layout.topMargin: Theme.s12
                                        Layout.bottomMargin: Theme.s12
                                        spacing: Theme.s2

                                        // The name is the row's one action: it
                                        // hands the CID up, never the contents.
                                        Text {
                                            text: modelData.name || ""
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.body
                                            font.weight: Theme.medium
                                            color: Theme.ink
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true

                                            Accessible.role: Accessible.Button
                                            Accessible.name: "Open " + (modelData.name || "")
                                            activeFocusOnTab: true

                                            Rectangle {
                                                anchors.fill: parent
                                                anchors.margins: -3
                                                radius: Theme.rControl
                                                color: "transparent"
                                                border.width: 2
                                                border.color: Theme.violet
                                                visible: parent.activeFocus
                                            }
                                            MouseArea {
                                                anchors.fill: parent
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: entry.openFile(modelData.cid || "")
                                            }
                                            Keys.onReturnPressed: entry.openFile(modelData.cid || "")
                                        }
                                        Text {
                                            text: (modelData.size || "") + " · " + (modelData.cid || "")
                                            font.family: Theme.mono
                                            font.pixelSize: Theme.meta
                                            color: Theme.ink3
                                            elide: Text.ElideMiddle
                                            Layout.fillWidth: true
                                        }
                                    }

                                    Badge {
                                        text: single
                                              ? (modelData.onThisNodeOnly ? "1 copy · only on this laptop" : "1 copy")
                                              : (modelData.copies || 0) + " copies"
                                        kind: single ? "attention" : "done"
                                    }

                                    // Amber outline: one copy is the thing that
                                    // needs attention, not the file itself.
                                    TintButton {
                                        visible: single
                                        label: "Pin to replica"
                                        accessibleName: "Pin " + (modelData.name || "") + " to replica"
                                        color: Theme.surface
                                        fg: Theme.amber
                                        edge: Theme.amber
                                        weight: Theme.semibold
                                        // TODO(phase1b): needs files.pin(cid);
                                        // the row's copy count updates on its reply.
                                        onClicked: console.log("[olb_ui] pin to replica:", modelData.cid)
                                    }
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    implicitHeight: 1
                                    color: Theme.lineSoft
                                    visible: index < entry.files.length - 1
                                }
                            }
                        }

                        Text {
                            text: "Files stay encrypted with the project key until you publish them."
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                    }
                }

                // ================= SIDE COLUMN =================
                ColumnLayout {
                    Layout.alignment: Qt.AlignTop
                    Layout.fillWidth: !entry.twoColumn
                    Layout.preferredWidth: entry.twoColumn ? 340 : -1
                    spacing: Theme.s16

                    // ---- Agent suggestions ----
                    Panel {
                        title: "Agent suggestions (" + entry.suggestions.length + ")"
                        Layout.fillWidth: true

                        Text {
                            text: (entry.agent.name || "") + " · own key · run by " + (entry.agent.operator || "")
                            visible: (entry.agent.name || "") !== ""
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }

                        Repeater {
                            model: entry.suggestions
                            delegate: Rectangle {
                                required property var modelData
                                Layout.fillWidth: true
                                radius: Theme.rInner
                                color: Theme.surface
                                border.width: 1
                                border.color: Theme.line
                                implicitHeight: suggestionBody.implicitHeight + Theme.s12 * 2

                                ColumnLayout {
                                    id: suggestionBody
                                    anchors.fill: parent
                                    anchors.margins: Theme.s12
                                    spacing: Theme.s8

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: Theme.s8
                                        FieldLabel {
                                            text: modelData.label || ""
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        Badge {
                                            visible: !!modelData.inferred
                                            text: "Inferred · confirm"
                                            kind: "attention"
                                        }
                                    }

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: Theme.s2

                                        // Removed value in amber, added value in
                                        // accent: the diff's meaning, not decoration.
                                        Text {
                                            text: modelData.before ? "− " + modelData.before : "(empty)"
                                            font.family: Theme.mono
                                            font.pixelSize: Theme.meta
                                            color: modelData.before ? Theme.amber : Theme.ink3
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        Text {
                                            text: "+ " + (modelData.after || "")
                                            font.family: Theme.mono
                                            font.pixelSize: Theme.meta
                                            color: Theme.accent
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                    }

                                    Text {
                                        visible: (modelData.source || "") !== ""
                                        text: modelData.source || ""
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink2
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: Theme.s8
                                        TintButton {
                                            label: "Accept"
                                            accessibleName: "Accept suggestion: " + (modelData.label || "")
                                            color: Theme.accentTint
                                            fg: Theme.accent
                                            edge: Theme.accent
                                            weight: Theme.semibold
                                            // TODO(phase1b): needs
                                            // agent.applySuggestion(recordId, suggestionId).
                                            onClicked: console.log("[olb_ui] accept suggestion:", modelData.id)
                                        }
                                        TintButton {
                                            label: "Reject"
                                            accessibleName: "Reject suggestion: " + (modelData.label || "")
                                            color: Theme.surface
                                            fg: Theme.ink
                                            // TODO(phase1b): needs
                                            // agent.rejectSuggestion(recordId, suggestionId).
                                            onClicked: console.log("[olb_ui] reject suggestion:", modelData.id)
                                        }
                                        Item { Layout.fillWidth: true }
                                    }
                                }
                            }
                        }

                        Text {
                            text: "Nothing changes until you accept. Accepted edits are credited to the agent, not to you."
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                    }

                    // ---- Provenance ----
                    Panel {
                        title: "Provenance"
                        Layout.fillWidth: true

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s6
                            visible: (entry.provenance.builtOn || []).length > 0

                            FieldLabel { text: "Built on" }
                            // TODO(phase1b): same as the Follows chips — these
                            // become links once the shell can open a record.
                            Repeater {
                                model: entry.provenance.builtOn || []
                                delegate: Text {
                                    required property var modelData
                                    text: modelData
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.body
                                    color: Theme.ink
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s6
                            visible: (entry.provenance.usedBy || []).length > 0

                            FieldLabel { text: "Used by" }
                            Repeater {
                                model: entry.provenance.usedBy || []
                                delegate: Text {
                                    required property var modelData
                                    text: modelData
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.body
                                    color: Theme.ink
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: 1
                            color: Theme.lineSoft
                            visible: entry.signatures.length > 0
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s6
                            visible: entry.signatures.length > 0

                            FieldLabel { text: "Signatures" }
                            Repeater {
                                model: entry.signatures
                                delegate: Text {
                                    required property var modelData
                                    readonly property bool waiting: modelData.state !== "signed"
                                    text: (modelData.name || "") + " · " + (modelData.role || "")
                                          + " · " + (modelData.state || "")
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.body
                                    // Amber while an approver has not signed yet.
                                    color: waiting ? Theme.amber : Theme.ink
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: 1
                            color: Theme.lineSoft
                            visible: (entry.anchor.current || "") !== ""
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s4
                            visible: (entry.anchor.current || "") !== ""

                            FieldLabel { text: "On-chain anchor" }
                            Text {
                                text: entry.anchor.current || ""
                                font.family: Theme.sans
                                font.pixelSize: Theme.body
                                color: Theme.ink
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }
                            Text {
                                visible: (entry.anchor.previous || "") !== ""
                                text: entry.anchor.previous || ""
                                font.family: Theme.sans
                                font.pixelSize: Theme.meta
                                color: Theme.ink3
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }
                            LinkRow {
                                visible: (entry.anchor.previous || "") !== ""
                                label: "Verify proof"
                                // TODO(phase1b): needs
                                // record.verifyAnchor(recordId, version) —
                                // recomputing the Merkle proof against olb_registry.
                                onClicked: console.log("[olb_ui] verify anchor proof:", entry.recordId)
                            }
                        }
                    }

                    // ---- Versions ----
                    Panel {
                        title: "Versions"
                        Layout.fillWidth: true

                        Repeater {
                            model: entry.versions
                            delegate: RowLayout {
                                required property var modelData
                                readonly property bool isHead: modelData.v === entry.record.version
                                Layout.fillWidth: true
                                spacing: Theme.s8

                                Text {
                                    text: "v" + (modelData.v || "") + " · " + (modelData.by || "")
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.body
                                    font.weight: isHead ? Theme.medium : Theme.regular
                                    color: Theme.ink
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: modelData.time || ""
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                }
                                LinkRow {
                                    visible: !isHead
                                    label: "Compare"
                                    fg: Theme.violet
                                    onClicked: entry.openHistory(entry.recordId)
                                }
                            }
                        }

                        Text {
                            text: "Versions are never overwritten."
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }
    }
}
