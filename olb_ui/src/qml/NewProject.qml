import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S8 · New project (docs/03). Creating one writes a signed commit.
Item {
    id: screen
    property var backend: null
    signal created()

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
                    text: "New project"
                    font.family: Theme.sans
                    font.pixelSize: Theme.pageTitle
                    font.weight: Theme.semibold
                    font.letterSpacing: Theme.titleSpacing
                    color: Theme.ink
                }
                Text {
                    text: "A project groups ideas as they move through the six stages."
                    font.family: Theme.sans
                    font.pixelSize: Theme.body
                    color: Theme.ink3
                }

                Panel {
                    title: "Details"
                    Layout.fillWidth: true

                    Text {
                        text: "Title"
                        font.family: Theme.sans
                        font.pixelSize: Theme.small
                        font.weight: Theme.medium
                        color: Theme.ink2
                    }
                    TextField {
                        id: titleField
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        placeholderText: "What is this project about?"
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        Accessible.name: "Project title"
                    }

                    Text {
                        text: "Summary"
                        font.family: Theme.sans
                        font.pixelSize: Theme.small
                        font.weight: Theme.medium
                        color: Theme.ink2
                    }
                    TextArea {
                        id: summaryField
                        Layout.fillWidth: true
                        Layout.preferredHeight: 96
                        placeholderText: "One or two sentences."
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        Accessible.name: "Project summary"
                    }

                    Text {
                        text: "Visibility"
                        font.family: Theme.sans
                        font.pixelSize: Theme.small
                        font.weight: Theme.medium
                        color: Theme.ink2
                    }
                    ComboBox {
                        id: privacyField
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        model: ["Private · project group", "Private · only you", "Public"]
                        Accessible.name: "Project visibility"
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s12
                        Item { Layout.fillWidth: true }
                        Button {
                            text: "Create project"
                            implicitHeight: Theme.touchTarget
                            highlighted: true
                            enabled: titleField.text.trim().length > 0 && screen.backend
                            onClicked: {
                                screen.backend.createProject(titleField.text,
                                                             summaryField.text,
                                                             privacyField.currentText);
                                titleField.text = "";
                                summaryField.text = "";
                                screen.created();
                            }
                        }
                    }
                }

                // Live preview, as docs/03 S8 calls for.
                Panel {
                    title: "Preview"
                    Layout.fillWidth: true
                    Text {
                        text: titleField.text.trim().length > 0 ? titleField.text : "Untitled project"
                        font.family: Theme.sans
                        font.pixelSize: Theme.cardTitle
                        font.weight: Theme.semibold
                        color: Theme.ink
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                    }
                    Text {
                        visible: summaryField.text.length > 0
                        text: summaryField.text
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        color: Theme.ink2
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                    }
                    Badge { text: privacyField.currentText; kind: "neutral" }
                }
            }
        }
    }
}
