import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S3 · Pre-register a hypothesis (docs/03, design/canvas/Hypothesis.dc.html).
//
// Lock the plan before the data exists: the group writes it, reviews it, and
// commits a salted fingerprint to LEZ while the text stays encrypted with the
// project key. Everything is rendered from `hypothesesData`, whose shape is
// fixtures/sample-project.json → hypotheses.h1.
//
// The commit itself is Phase 2 — the olb_registry program and
// hypothesis.commit do not exist yet — so "Commit to LEZ" is built for real,
// obeys its gating rule, and emits commitRequested() for the shell to wire.
// It never fakes a success state.
Item {
    id: screen

    // ---- Interface contract ----
    property var hypothesesData: ({})
    signal commitRequested(string hypothesisId, bool sealedCopyForReviewers)
    signal draftSaveRequested(string hypothesisId, var plan)
    signal previewRequested(string hypothesisId, var plan)
    signal openRecord(string recordId)
    signal openBoard()

    // ---- Derived, all guarded: the record can be empty or arrive late ----
    readonly property var record: screen.hypothesesData || ({})
    readonly property string hypothesisId: screen.record.id || ""
    readonly property string label: screen.record.label || "H1"
    readonly property var steps: screen.record.steps || []
    readonly property var commitment: screen.record.commitment || ({})
    readonly property var publicItems: screen.commitment["public"] || []
    readonly property var privateItems: screen.commitment["private"] || []
    readonly property string fingerprint: screen.commitment.fingerprintPreview || ""

    // `growsFrom` may be a plain label (records.hyp-h1) or carry a record id.
    readonly property var growsFrom: {
        var g = screen.record.growsFrom;
        if (!g) return ({ label: "", recordId: "" });
        if (typeof g === "string") return ({ label: g, recordId: "" });
        return ({ label: g.label || "", recordId: g.recordId || "" });
    }
    // `review` may be a plain summary string or the full object.
    readonly property var review: {
        var r = screen.record.review;
        if (!r) return ({ summary: "", complete: false });
        if (typeof r === "string") return ({ summary: r, complete: false });
        return r;
    }
    readonly property bool reviewComplete: screen.review.complete === true

    readonly property bool twoColumn: screen.width > Theme.narrowBreakpoint

    // ---- Local editing state ----
    // Predictions are editable, so they live here rather than in the binding.
    // Edits write back into the array in place: reassigning the property would
    // rebuild the delegates and take the caret with it.
    property var predictions: []
    property bool sealedCopy: false
    property bool understood: false

    function seed() {
        // Read through hypothesesData, not the derived properties: this runs
        // from onHypothesesDataChanged, which can fire before they are bound.
        var data = screen.hypothesesData || ({});
        var src = data.predictions || [];
        var out = [];
        for (var i = 0; i < src.length; i++)
            out.push({ label: src[i].label || ("P" + (i + 1)), text: src[i].text || "" });
        screen.predictions = out;
        screen.sealedCopy = (data.commitment || ({})).sealedCopyForReviewers === true;
        screen.understood = false;
    }
    function setPrediction(i, value) {
        if (i >= 0 && i < screen.predictions.length)
            screen.predictions[i].text = value;
    }
    function addPrediction() {
        screen.predictions = screen.predictions.concat(
            [{ label: "P" + (screen.predictions.length + 1), text: "" }]);
        previewTimer.restart();
    }
    function plan() {
        var texts = [];
        for (var i = 0; i < screen.predictions.length; i++)
            texts.push({ label: screen.predictions[i].label,
                         text: screen.predictions[i].text });
        return ({
            statement: statementField.text,
            predictions: texts,
            falsification: falsificationField.text,
            analysisPlan: analysisField.text
        });
    }

    onHypothesesDataChanged: screen.seed()
    Component.onCompleted: screen.seed()

    // A fingerprint is only meaningful for the text that is on screen, so an
    // edit asks the core for a fresh preview (hypothesis.previewCommitment,
    // docs/03 S3). Debounced like the entry-screen draft save.
    Timer {
        id: previewTimer
        interval: 800
        repeat: false
        onTriggered: screen.previewRequested(screen.hypothesisId, screen.plan())
    }

    // ---- A 32 px pill that links to another record (docs/02 §4 ChipLink) ----
    component ChipLink: Rectangle {
        id: chip
        property string label: ""
        signal clicked()

        implicitHeight: Theme.chipHeight
        implicitWidth: chipText.implicitWidth + Theme.s12 * 2
        radius: Theme.rPill
        color: Theme.ground
        border.width: 1
        border.color: Theme.line

        Accessible.role: Accessible.Link
        Accessible.name: chip.label
        Accessible.onPressAction: chip.clicked()
        activeFocusOnTab: true

        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: Theme.rPill
            color: "transparent"
            border.width: 2
            border.color: Theme.violet
            visible: parent.activeFocus
        }
        Text {
            id: chipText
            anchors.centerIn: parent
            text: chip.label
            font.family: Theme.sans
            font.pixelSize: Theme.body
            color: Theme.ink
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: chip.clicked()
        }
        Keys.onReturnPressed: chip.clicked()
        Keys.onSpacePressed: chip.clicked()
    }

    // ---- A labelled list inside the commit panel ----
    component CommitList: ColumnLayout {
        id: list
        property string heading: ""
        property color headingColor: Theme.violet
        property var items: []

        spacing: Theme.s8

        Text {
            text: list.heading
            font.family: Theme.sans
            font.pixelSize: Theme.meta
            font.weight: Theme.semibold
            color: list.headingColor
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        Repeater {
            model: list.items
            delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s6
                spacing: Theme.s8
                Text {
                    text: "•"
                    font.family: Theme.sans
                    font.pixelSize: Theme.body
                    color: Theme.ink3
                    Layout.alignment: Qt.AlignTop
                    Accessible.ignored: true
                }
                Text {
                    text: modelData
                    font.family: Theme.sans
                    font.pixelSize: Theme.body
                    lineHeight: Theme.bodyLineHeight
                    color: Theme.ink2
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
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
                    text: "Hypotheses"
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
                    text: screen.label
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink
                }
                Item { Layout.fillWidth: true }
            }

            // ---- Header ----
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.maximumWidth: 760            // design/canvas header max-width
                spacing: Theme.s8

                Text {
                    text: "Pre-register " + screen.label
                    font.family: Theme.sans
                    font.pixelSize: Theme.pageTitle
                    font.weight: Theme.semibold
                    font.letterSpacing: Theme.titleSpacing
                    color: Theme.ink
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                Text {
                    text: "Lock the plan before the data arrives. Only a fingerprint goes "
                          + "on-chain; the text stays encrypted with the project key until "
                          + "you reveal it."
                    font.family: Theme.sans
                    font.pixelSize: Theme.lede
                    lineHeight: Theme.bodyLineHeight
                    color: Theme.ink2
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            // ---- Steps ----
            Stepper {
                steps: screen.steps
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
            }

            // ---- The plan, and what gets committed ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s48
                columns: screen.twoColumn ? 2 : 1
                columnSpacing: Theme.s20
                rowSpacing: Theme.s20

                // ================= The plan =================
                Panel {
                    title: "The plan"
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s6

                        FieldLabel { text: "Hypothesis" }
                        TextArea {
                            id: statementField
                            Layout.fillWidth: true
                            Layout.preferredHeight: 92
                            text: screen.record.statement || ""
                            wrapMode: TextArea.Wrap
                            font.family: Theme.sans
                            font.pixelSize: Theme.lede
                            color: Theme.ink
                            Accessible.name: "Hypothesis"
                            onTextChanged: previewTimer.restart()
                        }
                    }

                    // ---- Predictions ----
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: Theme.s6
                        spacing: Theme.s8

                        FieldLabel { text: "Predictions" }

                        Repeater {
                            model: screen.predictions
                            delegate: RowLayout {
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true
                                spacing: Theme.s10

                                Text {
                                    text: modelData.label
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.meta
                                    color: Theme.violet
                                    Layout.preferredWidth: Theme.s24
                                }
                                TextField {
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    text: modelData.text
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.body
                                    Accessible.name: "Prediction " + (index + 1)
                                    onTextChanged: {
                                        screen.setPrediction(index, text);
                                        previewTimer.restart();
                                    }
                                }
                            }
                        }

                        TextLink {
                            label: "+ Add prediction"
                            Layout.leftMargin: Theme.s24 + Theme.s10
                            accessibleName: "Add prediction"
                            onClicked: screen.addPrediction()
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s6

                        FieldLabel { text: "What would prove it wrong" }
                        TextArea {
                            id: falsificationField
                            Layout.fillWidth: true
                            Layout.preferredHeight: 72
                            text: screen.record.falsification || ""
                            wrapMode: TextArea.Wrap
                            font.family: Theme.sans
                            font.pixelSize: Theme.body
                            color: Theme.ink
                            Accessible.name: "What would prove it wrong"
                            onTextChanged: previewTimer.restart()
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s6

                        FieldLabel { text: "Analysis plan" }
                        TextArea {
                            id: analysisField
                            Layout.fillWidth: true
                            Layout.preferredHeight: 92
                            text: screen.record.analysisPlan || ""
                            wrapMode: TextArea.Wrap
                            font.family: Theme.sans
                            font.pixelSize: Theme.body
                            color: Theme.ink
                            Accessible.name: "Analysis plan"
                            onTextChanged: previewTimer.restart()
                        }
                    }

                    Flow {
                        Layout.fillWidth: true
                        Layout.topMargin: Theme.s2
                        spacing: Theme.s10

                        FieldLabel {
                            text: "Grows from"
                            height: Theme.chipHeight
                            verticalAlignment: Text.AlignVCenter
                        }
                        ChipLink {
                            label: screen.growsFrom.label
                            visible: screen.growsFrom.label !== ""
                            onClicked: if (screen.growsFrom.recordId !== "")
                                           screen.openRecord(screen.growsFrom.recordId)
                        }
                    }

                    // ---- Group review ----
                    Rectangle {
                        Layout.fillWidth: true
                        Layout.topMargin: Theme.s14
                        height: 1
                        color: Theme.lineSoft
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s4

                        FieldLabel { text: "Group review" }
                        Text {
                            text: screen.review.summary || "Not reviewed yet"
                            font.family: Theme.sans
                            font.pixelSize: Theme.body
                            color: Theme.ink
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                    }
                }

                // ================= What gets committed =================
                Panel {
                    id: commitPanel
                    title: "What gets committed"
                    // Violet 2 px: this panel is about the commitment itself.
                    border.width: 2
                    border.color: Theme.violet
                    Layout.fillWidth: !screen.twoColumn
                    Layout.preferredWidth: screen.twoColumn ? 340 : -1
                    Layout.alignment: Qt.AlignTop

                    CommitList {
                        heading: "Public, on LEZ"
                        headingColor: Theme.violet
                        items: screen.publicItems
                        Layout.fillWidth: true
                    }
                    CommitList {
                        heading: "Private, stays with the group"
                        headingColor: Theme.slate
                        items: screen.privateItems
                        Layout.fillWidth: true
                    }

                    // ---- Fingerprint preview ----
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: fingerprintCol.implicitHeight + Theme.s12 * 2
                        radius: Theme.rControl
                        color: Theme.ground

                        ColumnLayout {
                            id: fingerprintCol
                            anchors.fill: parent
                            anchors.margins: Theme.s12
                            spacing: Theme.s4

                            Text {
                                text: "Fingerprint preview"
                                font.family: Theme.sans
                                font.pixelSize: Theme.meta
                                color: Theme.ink3
                                Layout.fillWidth: true
                            }
                            Text {
                                text: screen.fingerprint
                                font.family: Theme.mono
                                font.pixelSize: Theme.fingerprint
                                font.letterSpacing: 0.56     // 0.04em at 14 px
                                color: Theme.ink
                                wrapMode: Text.WrapAnywhere
                                Layout.fillWidth: true
                                Accessible.role: Accessible.StaticText
                                Accessible.name: "Fingerprint preview " + screen.fingerprint
                            }
                        }
                    }

                    CheckRow {
                        title: "Share a sealed copy with the funding reviewers"
                        checked: screen.sealedCopy
                        indicatorColor: Theme.violet
                        pad: Theme.s4
                        Layout.fillWidth: true
                        onToggled: screen.sealedCopy = !screen.sealedCopy
                    }
                    CheckRow {
                        title: "I understand that edits after this point create a new "
                               + "version, and this one stays on record"
                        checked: screen.understood
                        indicatorColor: Theme.violet
                        pad: Theme.s4
                        Layout.fillWidth: true
                        onToggled: screen.understood = !screen.understood
                    }

                    Flow {
                        Layout.fillWidth: true
                        spacing: Theme.s8

                        ChainButton {
                            text: "Commit to LEZ"
                            // docs/03 S3: blocked until the acknowledgement is
                            // ticked and group review is complete.
                            enabled: screen.understood && screen.reviewComplete
                            disabledReason: !screen.reviewComplete
                                ? "Group review is not complete yet"
                                : "Tick the acknowledgement first"
                            onClicked: screen.commitRequested(screen.hypothesisId,
                                                              screen.sealedCopy)
                        }
                        PrimaryButton {
                            text: "Save draft"
                            primary: false
                            onClicked: screen.draftSaveRequested(screen.hypothesisId,
                                                                 screen.plan())
                        }
                    }

                    Text {
                        text: "When you publish, the salt is released so anyone can "
                              + "recompute the fingerprint and confirm the plan predates "
                              + "the data."
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        lineHeight: Theme.bodyLineHeight
                        color: Theme.ink3
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
