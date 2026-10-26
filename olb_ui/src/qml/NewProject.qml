import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S8 · New project, following design/canvas and the supplied mockup:
// six numbered sections, a live board preview and a "what happens" panel.
Item {
    id: screen
    property var backend: null
    signal created()
    signal cancelled()

    // ---- form state ----
    property var stageOn: [true, true, true, true, true, true]
    readonly property var stageDefs: [
        { n: "1 · Idea",          d: "Capture the question and why it matters, timestamped privately." },
        { n: "2 · Hypothesis",    d: "Pre-register predictions and an analysis plan before data arrives." },
        { n: "3 · Funding",       d: "Raise funds into escrow, released on approved milestones." },
        { n: "4 · Experiments",   d: "Notebook entries, protocols and failed runs, all versioned." },
        { n: "5 · Data and results", d: "Datasets with RO-Crate manifests, kept on replica nodes." },
        { n: "6 · Publishing",    d: "Bundle, reveal commitments and publish with full provenance." }
    ]
    readonly property var stageNames: ["Idea", "Hypothesis", "Funding", "Experiments", "Data", "Publish"]
    property string privacy: "Private"
    property string anchoring: "Batched every few minutes"
    property string teamSize: "2 to 5 people"
    property var team: []
    property var references: []

    readonly property var selectedStages: {
        var out = [];
        for (var i = 0; i < stageOn.length; i++) if (stageOn[i]) out.push(stageNames[i]);
        return out;
    }
    function setStage(i, v) {
        var copy = stageOn.slice(); copy[i] = v; stageOn = copy;
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: screen.width
            spacing: Theme.s16

            // ---- breadcrumb + heading ----
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.topMargin: Theme.s24
                spacing: Theme.s8

                Text {
                    text: "Projects  /  New project"
                    font.family: Theme.sans
                    font.pixelSize: Theme.meta
                    color: Theme.ink3
                }
                Text {
                    text: "New project"
                    font.family: Theme.sans
                    font.pixelSize: Theme.pageTitle
                    font.weight: Theme.semibold
                    font.letterSpacing: Theme.titleSpacing
                    color: Theme.ink
                }
                Text {
                    text: "Set up who can see the work, which research stages it will use and how its "
                          + "history is recorded. You can change everything except privacy later."
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink3
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            // ---- two columns: form | side panels ----
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s32
                spacing: Theme.s20
                Layout.alignment: Qt.AlignTop

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s16

                    // ---- 1 Basics ----
                    FormSection {
                        index: 1; title: "Basics"
                        Layout.fillWidth: true

                        FieldLabel { text: "Title" }
                        TextField {
                            id: titleField
                            Layout.fillWidth: true
                            implicitHeight: Theme.touchTarget
                            placeholderText: "e.g. Enzyme stability under industrial conditions"
                            font.family: Theme.sans; font.pixelSize: Theme.body
                            Accessible.name: "Title"
                        }

                        FieldLabel { text: "One-line summary" }
                        TextField {
                            id: summaryField
                            Layout.fillWidth: true
                            implicitHeight: Theme.touchTarget
                            placeholderText: "What question does this project answer?"
                            font.family: Theme.sans; font.pixelSize: Theme.body
                            Accessible.name: "One-line summary"
                        }

                        FieldLabel { text: "Description" }
                        TextArea {
                            id: descField
                            Layout.fillWidth: true
                            Layout.preferredHeight: 104
                            placeholderText: "Background, aims and the approach you plan to take."
                            font.family: Theme.sans; font.pixelSize: Theme.body
                            Accessible.name: "Description"
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s12
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                FieldLabel { text: "Start date" }
                                TextField {
                                    id: startField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    placeholderText: "dd/mm/yyyy"
                                    font.family: Theme.sans; font.pixelSize: Theme.body
                                    Accessible.name: "Start date"
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                FieldLabel { text: "Expected duration" }
                                ComboBox {
                                    id: durationField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    model: ["Under 6 months", "6 to 12 months", "1 to 2 years", "Over 2 years"]
                                    currentIndex: 1
                                    Accessible.name: "Expected duration"
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                FieldLabel { text: "Default licence on publication" }
                                ComboBox {
                                    id: licenceField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    model: ["CC BY 4.0", "CC BY-SA 4.0", "CC0", "All rights reserved"]
                                    Accessible.name: "Default licence"
                                }
                            }
                        }
                    }

                    // ---- 2 Field and team ----
                    FormSection {
                        index: 2; title: "Field and team"
                        Layout.fillWidth: true

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s12
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                FieldLabel { text: "Research field" }
                                ComboBox {
                                    id: fieldField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    model: ["Biochemistry and enzymology", "Molecular biology",
                                            "Materials science", "Computer science", "Other"]
                                    Accessible.name: "Research field"
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                FieldLabel { text: "Keywords" }
                                TextField {
                                    id: keywordsField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    placeholderText: "Separate with commas"
                                    font.family: Theme.sans; font.pixelSize: Theme.body
                                    Accessible.name: "Keywords"
                                }
                            }
                        }

                        FieldLabel { text: "Team size" }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Theme.s8
                            Repeater {
                                model: ["Just me", "2 to 5 people", "6 to 15 people", "More than 15"]
                                delegate: Rectangle {
                                    required property string modelData
                                    readonly property bool on: screen.teamSize === modelData
                                    height: Theme.touchTarget
                                    width: sizeLabel.implicitWidth + Theme.s16 * 2
                                    radius: Theme.rControl
                                    color: on ? Theme.ink : Theme.surface
                                    border.width: 1
                                    border.color: on ? Theme.ink : Theme.inputBorder
                                    Accessible.role: Accessible.RadioButton
                                    Accessible.name: modelData
                                    Text {
                                        id: sizeLabel
                                        anchors.centerIn: parent
                                        text: modelData
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.small
                                        color: parent.on ? Theme.textOnAccent : Theme.ink2
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: screen.teamSize = modelData
                                    }
                                }
                            }
                        }

                        FieldLabel { text: "Team members" }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s8
                            TextField {
                                id: memberField
                                Layout.fillWidth: true
                                implicitHeight: Theme.touchTarget
                                placeholderText: "Name, ORCID or Logos key"
                                font.family: Theme.sans; font.pixelSize: Theme.body
                                Accessible.name: "Team member"
                            }
                            ComboBox {
                                id: roleField
                                Layout.preferredWidth: 160
                                implicitHeight: Theme.touchTarget
                                model: ["Contributor", "Reviewer", "Lead"]
                                Accessible.name: "Member role"
                            }
                            Button {
                                text: "Add"
                                implicitHeight: Theme.touchTarget
                                enabled: memberField.text.trim().length > 0
                                onClicked: {
                                    var t = screen.team.slice();
                                    t.push({ who: memberField.text.trim(), role: roleField.currentText });
                                    screen.team = t;
                                    memberField.text = "";
                                }
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s2
                            Text {
                                text: "You · owner"
                                font.family: Theme.sans
                                font.pixelSize: Theme.small
                                font.weight: Theme.medium
                                color: Theme.ink
                            }
                            Text {
                                text: "Lead · this node's signing key"
                                font.family: Theme.sans
                                font.pixelSize: Theme.meta
                                color: Theme.ink3
                            }
                        }

                        Repeater {
                            model: screen.team
                            delegate: RowLayout {
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true
                                spacing: Theme.s8
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s2
                                    Text {
                                        text: modelData.who
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.small
                                        font.weight: Theme.medium
                                        color: Theme.ink
                                    }
                                    Text {
                                        text: modelData.role + " · invite sent over Logos Chat when you create the project"
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                    }
                                }
                                Button {
                                    text: "Remove"
                                    implicitHeight: Theme.touchTarget
                                    onClicked: {
                                        var t = screen.team.slice();
                                        t.splice(index, 1);
                                        screen.team = t;
                                    }
                                }
                            }
                        }
                    }

                    // ---- 3 Research stages ----
                    FormSection {
                        index: 3; title: "Research stages"
                        hint: "Pick the stages this project will use. Skipped stages are hidden from the board and can be turned on later."
                        Layout.fillWidth: true

                        GridLayout {
                            Layout.fillWidth: true
                            columns: screen.width > 980 ? 2 : 1
                            columnSpacing: Theme.s12
                            rowSpacing: Theme.s12
                            Repeater {
                                model: screen.stageDefs
                                delegate: ChoiceCard {
                                    required property var modelData
                                    required property int index
                                    Layout.fillWidth: true
                                    title: modelData.n
                                    body: modelData.d
                                    checked: screen.stageOn[index]
                                    onToggled: screen.setStage(index, !screen.stageOn[index])
                                }
                            }
                        }
                    }

                    // ---- 4 Privacy ----
                    FormSection {
                        index: 4; title: "Privacy"
                        hint: "This sets how records are stored and encrypted, so it cannot be changed after the project is created."
                        Layout.fillWidth: true

                        ChoiceCard {
                            Layout.fillWidth: true
                            radio: true
                            title: "Open"
                            body: "Anyone can find and read the project. Records become public as soon as they are shared with the group."
                            note: "Content is published to Logos Storage unencrypted; commits are anchored openly."
                            checked: screen.privacy === "Open"
                            onToggled: screen.privacy = "Open"
                        }
                        ChoiceCard {
                            Layout.fillWidth: true
                            radio: true
                            title: "Private"
                            body: "Only members can read anything. You choose what to reveal when you publish."
                            note: "Content is encrypted with the project key; only fingerprints go on-chain."
                            checked: screen.privacy === "Private"
                            onToggled: screen.privacy = "Private"
                        }
                        ChoiceCard {
                            Layout.fillWidth: true
                            radio: true
                            title: "Invite only"
                            body: "The title, summary and field are listed so others can ask to join. Everything else stays private."
                            note: "The listing is public; records are encrypted like a private project."
                            checked: screen.privacy === "Invite only"
                            onToggled: screen.privacy = "Invite only"
                        }
                    }

                    // ---- 5 Version history and commits ----
                    FormSection {
                        index: 5; title: "Version history and commits"
                        hint: "Every save is already a signed commit on your machine. This sets how often commits are anchored on LEZ, which is what makes their timestamps provable to outsiders."
                        Layout.fillWidth: true

                        FieldLabel { text: "Anchoring frequency" }
                        GridLayout {
                            Layout.fillWidth: true
                            columns: screen.width > 980 ? 2 : 1
                            columnSpacing: Theme.s12
                            rowSpacing: Theme.s12

                            ChoiceCard {
                                Layout.fillWidth: true; radio: true
                                title: "Every change"
                                body: "Each commit is anchored on its own. Most precise timestamps, highest network fees."
                                checked: screen.anchoring === "Every change"
                                onToggled: screen.anchoring = "Every change"
                            }
                            ChoiceCard {
                                Layout.fillWidth: true; radio: true
                                title: "Batched every few minutes"
                                body: "Recommended. Commits are grouped; any single one can still be proven."
                                checked: screen.anchoring === "Batched every few minutes"
                                onToggled: screen.anchoring = "Batched every few minutes"
                            }
                            ChoiceCard {
                                Layout.fillWidth: true; radio: true
                                title: "Once a day"
                                body: "Lowest cost. Timestamps are provable to the day."
                                checked: screen.anchoring === "Once a day"
                                onToggled: screen.anchoring = "Once a day"
                            }
                            ChoiceCard {
                                Layout.fillWidth: true; radio: true
                                title: "Only at key moments"
                                body: "At pre-registration, milestone releases and publication."
                                checked: screen.anchoring === "Only at key moments"
                                onToggled: screen.anchoring = "Only at key moments"
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s12
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                FieldLabel { text: "Approvals needed to merge into main" }
                                ComboBox {
                                    id: approvalsField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    model: ["No approval", "1 approval", "2 approvals"]
                                    currentIndex: 1
                                    Accessible.name: "Approvals needed"
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s4
                                FieldLabel { text: "Replica node for project files" }
                                ComboBox {
                                    id: replicaField
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    model: ["Host lab replica", "No replica", "Choose later"]
                                    Accessible.name: "Replica node"
                                }
                            }
                        }

                        CheckBox {
                            id: branchesBox
                            checked: true
                            text: "Let members open branches to try alternatives without changing the main record"
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            Layout.fillWidth: true
                        }
                        CheckBox {
                            id: autoTagBox
                            checked: true
                            text: "Tag a version automatically at pre-registration, milestone releases and publication"
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            Layout.fillWidth: true
                        }
                    }

                    // ---- 6 References and links ----
                    FormSection {
                        index: 6; title: "References and links"
                        hint: "Prior work this project builds on. References become parent links in the provenance chain."
                        Layout.fillWidth: true

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s8
                            ComboBox {
                                id: refType
                                Layout.preferredWidth: 160
                                implicitHeight: Theme.touchTarget
                                model: ["Paper (DOI)", "Preprint", "Protocol", "Dataset", "Open Labbook record"]
                                Accessible.name: "Reference type"
                            }
                            TextField {
                                id: refValue
                                Layout.fillWidth: true
                                implicitHeight: Theme.touchTarget
                                placeholderText: "DOI, URL or Open Labbook record"
                                font.family: Theme.sans; font.pixelSize: Theme.body
                                Accessible.name: "Reference"
                            }
                            Button {
                                text: "Add"
                                implicitHeight: Theme.touchTarget
                                enabled: refValue.text.trim().length > 0
                                onClicked: {
                                    var r = screen.references.slice();
                                    r.push({ kind: refType.currentText, value: refValue.text.trim() });
                                    screen.references = r;
                                    refValue.text = "";
                                }
                            }
                        }

                        Repeater {
                            model: screen.references
                            delegate: RowLayout {
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s2
                                    Text {
                                        text: modelData.value
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.small
                                        font.weight: Theme.medium
                                        color: Theme.ink
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        text: modelData.kind
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                    }
                                }
                                Button {
                                    text: "Remove"
                                    implicitHeight: Theme.touchTarget
                                    onClicked: {
                                        var r = screen.references.slice();
                                        r.splice(index, 1);
                                        screen.references = r;
                                    }
                                }
                            }
                        }

                        FieldLabel { text: "Existing grant or funder (optional)" }
                        TextField {
                            id: grantField
                            Layout.fillWidth: true
                            implicitHeight: Theme.touchTarget
                            placeholderText: "Funder name and grant number"
                            font.family: Theme.sans; font.pixelSize: Theme.body
                            Accessible.name: "Grant or funder"
                        }
                    }

                    // ---- footer ----
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s12
                        Item { Layout.fillWidth: true }
                        Button {
                            text: "Cancel"
                            implicitHeight: Theme.touchTarget
                            onClicked: screen.cancelled()
                        }
                        PrimaryButton {
                            text: "Create project"
                            enabled: titleField.text.trim().length > 0 && screen.backend
                            onClicked: {
                                var kw = keywordsField.text.split(",")
                                            .map(function (s) { return s.trim(); })
                                            .filter(function (s) { return s.length > 0; });
                                screen.backend.createProject(JSON.stringify({
                                    title: titleField.text,
                                    summary: summaryField.text,
                                    description: descField.text,
                                    startDate: startField.text,
                                    duration: durationField.currentText,
                                    licence: licenceField.currentText,
                                    field: fieldField.currentText,
                                    keywords: kw,
                                    teamSize: screen.teamSize,
                                    team: screen.team,
                                    stages: screen.selectedStages,
                                    privacy: screen.privacy,
                                    anchoring: screen.anchoring,
                                    approvals: approvalsField.currentText,
                                    replica: replicaField.currentText,
                                    allowBranches: branchesBox.checked,
                                    autoTag: autoTagBox.checked,
                                    references: screen.references,
                                    grant: grantField.text
                                }));
                                titleField.text = ""; summaryField.text = ""; descField.text = "";
                                screen.created();
                            }
                        }
                    }
                }

                // ---- right-hand panels ----
                ColumnLayout {
                    Layout.preferredWidth: 280
                    Layout.maximumWidth: 280
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s16
                    visible: screen.width > 900

                    Panel {
                        title: "Preview on the board"
                        Layout.fillWidth: true

                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: prev.implicitHeight + Theme.s16 * 2
                            radius: Theme.rInner
                            color: Theme.surface
                            border.width: 1
                            border.color: Theme.line

                            ColumnLayout {
                                id: prev
                                anchors.fill: parent
                                anchors.margins: Theme.s16
                                spacing: Theme.s8

                                RowLayout {
                                    Layout.fillWidth: true
                                    Badge {
                                        text: screen.selectedStages.length > 0
                                              ? "Stage 1 of " + screen.selectedStages.length + " · " + screen.selectedStages[0]
                                              : "No stages"
                                        kind: "current"
                                    }
                                    Item { Layout.fillWidth: true }
                                    Text {
                                        text: screen.privacy
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                    }
                                }
                                Text {
                                    text: titleField.text.trim().length > 0 ? titleField.text : "Untitled project"
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.small
                                    font.weight: Theme.semibold
                                    color: Theme.ink
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                                // Stage rail
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s4
                                    Repeater {
                                        model: screen.selectedStages
                                        delegate: ColumnLayout {
                                            required property string modelData
                                            required property int index
                                            Layout.fillWidth: true
                                            spacing: Theme.s2
                                            Rectangle {
                                                Layout.fillWidth: true
                                                height: 3
                                                radius: Theme.rProgress
                                                color: index === 0 ? Theme.violet : Theme.lineSoft
                                            }
                                            Text {
                                                text: modelData
                                                font.family: Theme.sans
                                                font.pixelSize: 9
                                                color: index === 0 ? Theme.violet : Theme.ink3
                                                elide: Text.ElideRight
                                                Layout.fillWidth: true
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Text {
                            text: "Anchoring on LEZ"
                            font.family: Theme.sans; font.pixelSize: Theme.meta; color: Theme.ink3
                        }
                        Text {
                            text: screen.anchoring
                            font.family: Theme.sans; font.pixelSize: Theme.small
                            font.weight: Theme.medium; color: Theme.ink
                            wrapMode: Text.Wrap; Layout.fillWidth: true
                        }
                        Text {
                            text: "Team"
                            font.family: Theme.sans; font.pixelSize: Theme.meta; color: Theme.ink3
                        }
                        Text {
                            text: screen.teamSize + " · " + (screen.team.length + 1) + " added so far"
                            font.family: Theme.sans; font.pixelSize: Theme.small
                            font.weight: Theme.medium; color: Theme.ink
                            wrapMode: Text.Wrap; Layout.fillWidth: true
                        }
                        Text {
                            text: "Stages"
                            font.family: Theme.sans; font.pixelSize: Theme.meta; color: Theme.ink3
                        }
                        Text {
                            text: screen.selectedStages.join(" → ")
                            font.family: Theme.sans; font.pixelSize: Theme.small
                            font.weight: Theme.medium; color: Theme.ink
                            wrapMode: Text.Wrap; Layout.fillWidth: true
                        }
                    }

                    Panel {
                        title: "What happens when you create it"
                        Layout.fillWidth: true
                        Repeater {
                            model: [
                                "Your node generates the project key and the first signed commit.",
                                "Invites go to members over Logos Chat, sealed to their keys.",
                                "The replica node starts keeping a copy of every file.",
                                "Nothing is anchored until the first batch; only a fingerprint will go on-chain."
                            ]
                            delegate: RowLayout {
                                required property string modelData
                                required property int index
                                Layout.fillWidth: true
                                spacing: Theme.s8
                                Text {
                                    text: (index + 1) + "."
                                    font.family: Theme.sans; font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                    Layout.alignment: Qt.AlignTop
                                }
                                Text {
                                    text: modelData
                                    font.family: Theme.sans; font.pixelSize: Theme.meta
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
    }
}
