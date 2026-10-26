import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// General profile settings. The signing key is shown but never editable —
// secrets never leave olb_core (CLAUDE.md engineering rules).
Item {
    id: screen
    property var backend: null
    property var profile: ({})
    property string coreStatus: ""

    readonly property var status: {
        try { return JSON.parse(screen.coreStatus); } catch (e) { return ({}); }
    }

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
                    text: "Settings"
                    font.family: Theme.sans
                    font.pixelSize: Theme.pageTitle
                    font.weight: Theme.semibold
                    font.letterSpacing: Theme.titleSpacing
                    color: Theme.ink
                }

                Panel {
                    title: "Profile"
                    subtitle: "Shown to other members"
                    Layout.fillWidth: true

                    Text {
                        text: "Name"
                        font.family: Theme.sans
                        font.pixelSize: Theme.small
                        font.weight: Theme.medium
                        color: Theme.ink2
                    }
                    TextField {
                        id: nameField
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        placeholderText: "Your name"
                        text: screen.profile.name || ""
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        Accessible.name: "Your name"
                    }

                    Text {
                        text: "ORCID"
                        font.family: Theme.sans
                        font.pixelSize: Theme.small
                        font.weight: Theme.medium
                        color: Theme.ink2
                    }
                    TextField {
                        id: orcidField
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        placeholderText: "0000-0000-0000-0000"
                        text: screen.profile.orcid || ""
                        font.family: Theme.mono
                        font.pixelSize: Theme.body
                        Accessible.name: "ORCID identifier"
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Item { Layout.fillWidth: true }
                        PrimaryButton {
                            text: "Save profile"
                            enabled: !!screen.backend
                            onClicked: screen.backend.saveProfile(nameField.text, orcidField.text)
                        }
                    }
                }

                Panel {
                    title: "Identity"
                    subtitle: "Your signing key"
                    Layout.fillWidth: true

                    Text {
                        text: screen.profile.fingerprint || "—"
                        font.family: Theme.mono
                        font.pixelSize: Theme.fingerprint
                        color: Theme.violet
                        wrapMode: Text.WrapAnywhere
                        Layout.fillWidth: true
                    }
                    Text {
                        text: "The private key never leaves olb_core, and it now persists across "
                              + "launches, so this node keeps one identity."
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        color: Theme.ink3
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Text {
                        text: "Stored unsealed at mode 0600 beside the store. Anyone who can read "
                              + "that file is this node. Sealing at rest is still to come."
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        color: Theme.amber
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }

                Panel {
                    title: "Storage"
                    subtitle: "Where this profile keeps its data"
                    Layout.fillWidth: true
                    Text {
                        text: screen.profile.profileDir || "—"
                        font.family: Theme.mono
                        font.pixelSize: Theme.meta
                        color: Theme.ink2
                        wrapMode: Text.WrapAnywhere
                        Layout.fillWidth: true
                    }
                    Text {
                        text: "Kept outside the Basecamp profile tree on purpose: "
                              + "`lgs basecamp launch` wipes that tree on every launch."
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        color: Theme.ink3
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }

                Panel {
                    title: "Services"
                    Layout.fillWidth: true
                    RowLayout {
                        spacing: Theme.s20
                        Badge {
                            text: "Delivery " + (screen.status.state || "unknown")
                            kind: screen.status.state === "online" ? "done" : "attention"
                        }
                        Badge {
                            text: "Storage " + (screen.status.storage || "unknown")
                            kind: screen.status.storage === "online" ? "done" : "attention"
                        }
                    }
                }
            }
        }
    }
}
