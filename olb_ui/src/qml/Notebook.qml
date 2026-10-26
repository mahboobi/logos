import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Personal Notebook. Private notes that are not part of any project, each
// promotable into a project idea entering at a chosen stage.
Item {
    id: screen
    property var backend: null
    property var notes: []
    property var projects: []

    // Promotion entry points. Funding and Publish are deliberately absent:
    // an idea does not begin life needing money or a paper.
    readonly property var promoteStages: ["Idea", "Hypothesis", "Experiments", "Data"]

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: screen.width
            spacing: Theme.s24

            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: Theme.s32
                spacing: Theme.s16

                Text {
                    text: "Personal Notebook"
                    font.family: Theme.sans
                    font.pixelSize: Theme.pageTitle
                    font.weight: Theme.semibold
                    font.letterSpacing: Theme.titleSpacing
                    color: Theme.ink
                }
                Text {
                    text: "Private to you. Promote a note into a project when it is ready to be shared."
                    font.family: Theme.sans
                    font.pixelSize: Theme.body
                    color: Theme.ink3
                }

                Panel {
                    title: "New note"
                    Layout.fillWidth: true

                    TextField {
                        id: noteTitle
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        placeholderText: "Note title"
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        Accessible.name: "Note title"
                    }
                    TextArea {
                        id: noteBody
                        Layout.fillWidth: true
                        Layout.preferredHeight: 96
                        placeholderText: "What are you thinking?"
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        Accessible.name: "Note body"
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Item { Layout.fillWidth: true }
                        PrimaryButton {
                            text: "Add note"
                            enabled: noteTitle.text.trim().length > 0 && screen.backend
                            onClicked: {
                                screen.backend.createNote(noteTitle.text, noteBody.text);
                                noteTitle.text = "";
                                noteBody.text = "";
                            }
                        }
                    }
                }

                Text {
                    text: screen.notes.length + (screen.notes.length === 1 ? " note" : " notes")
                    font.family: Theme.sans
                    font.pixelSize: Theme.sectionTitle
                    font.weight: Theme.semibold
                    color: Theme.ink
                }

                Rectangle {
                    visible: screen.notes.length === 0
                    Layout.fillWidth: true
                    implicitHeight: 96
                    radius: Theme.rPanel
                    color: "transparent"
                    border.width: 1
                    border.color: Theme.dashed
                    Text {
                        anchors.centerIn: parent
                        text: "No notes yet."
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        color: Theme.ink3
                    }
                }

                Repeater {
                    model: screen.notes
                    delegate: Panel {
                        required property var modelData
                        readonly property bool isPromoted: !!modelData.promoted
                        Layout.fillWidth: true
                        title: modelData.title || ""
                        subtitle: isPromoted
                                  ? "Promoted to " + modelData.promoted.stage
                                  : ""

                        Text {
                            visible: (modelData.body || "") !== ""
                            text: modelData.body
                            font.family: Theme.sans
                            font.pixelSize: Theme.body
                            color: Theme.ink2
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }

                        Badge {
                            visible: isPromoted
                            text: "Promoted · " + (modelData.promoted ? modelData.promoted.stage : "")
                            kind: "done"
                        }

                        // Promote: pick the target project and the entry stage.
                        RowLayout {
                            visible: !isPromoted
                            Layout.fillWidth: true
                            spacing: Theme.s12

                            Text {
                                text: "Promote into"
                                font.family: Theme.sans
                                font.pixelSize: Theme.small
                                color: Theme.ink2
                            }
                            ComboBox {
                                id: projectPick
                                Layout.preferredWidth: 240
                                implicitHeight: Theme.touchTarget
                                textRole: "title"
                                valueRole: "id"
                                model: screen.projects
                                Accessible.name: "Target project"
                            }
                            Text {
                                text: "at stage"
                                font.family: Theme.sans
                                font.pixelSize: Theme.small
                                color: Theme.ink2
                            }
                            ComboBox {
                                id: stagePick
                                Layout.preferredWidth: 180
                                implicitHeight: Theme.touchTarget
                                model: screen.promoteStages
                                Accessible.name: "Entry stage"
                            }
                            Item { Layout.fillWidth: true }
                            Button {
                                text: "Promote"
                                implicitHeight: Theme.touchTarget
                                enabled: screen.projects.length > 0 && screen.backend
                                onClicked: screen.backend.promoteNote(modelData.id,
                                                                      projectPick.currentValue,
                                                                      stagePick.currentText)
                            }
                        }

                        Text {
                            visible: screen.projects.length === 0 && !isPromoted
                            text: "Create a project first — a note is promoted into one."
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            color: Theme.amber
                        }
                    }
                }
            }
        }
    }
}
