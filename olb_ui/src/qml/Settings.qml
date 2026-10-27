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
    // {id, fingerprint} from olb_core. Public halves only.
    property var myId: ({})
    // Invites waiting on a decision.
    property var invites: []
    signal acceptInvite(string inviteId)

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
                    title: "Your Open Labbook ID"
                    subtitle: "Hand this to someone so they can invite you"
                    Layout.fillWidth: true

                    ColumnLayout {
                        width: parent.width
                        spacing: Theme.s12

                        FieldLabel { text: "Fingerprint" }
                        Text {
                            text: screen.myId.fingerprint || "—"
                            font.family: Theme.mono
                            font.pixelSize: Theme.fingerprint
                            color: Theme.ink
                            Accessible.name: "Your fingerprint, " + (screen.myId.fingerprint || "not available")
                        }
                        Text {
                            text: "Compare this with the other person out loud before you trust "
                                  + "an ID. A name in an ID is a claim; the fingerprint is the "
                                  + "only part that is checked."
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }

                        FieldLabel { text: "ID" }
                        TextArea {
                            id: idBox
                            text: screen.myId.id || ""
                            readOnly: true
                            wrapMode: TextArea.WrapAnywhere
                            selectByMouse: true
                            font.family: Theme.mono
                            font.pixelSize: Theme.meta
                            color: Theme.ink2
                            Layout.fillWidth: true
                            Layout.preferredHeight: 92
                            Accessible.name: "Your Open Labbook ID"
                        }
                        PrimaryButton {
                            text: "Copy my ID"
                            accessibleName: "Copy my Open Labbook ID"
                            enabled: (screen.myId.id || "") !== ""
                            onClicked: {
                                idBox.selectAll();
                                idBox.copy();
                                idBox.deselect();
                            }
                        }
                    }
                }

                Panel {
                    title: "Invitations"
                    subtitle: screen.invites.length === 0
                              ? "Nothing waiting"
                              : screen.invites.length + " waiting on you"
                    Layout.fillWidth: true
                    visible: screen.invites.length > 0

                    ColumnLayout {
                        width: parent.width
                        spacing: Theme.s16

                        Repeater {
                            model: screen.invites
                            delegate: ColumnLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: Theme.s8

                                Text {
                                    text: modelData.projectName || "A project"
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.cardTitle
                                    font.weight: Theme.semibold
                                    color: Theme.ink
                                }
                                Text {
                                    text: "From " + (modelData.inviterFingerprint || "an unknown key")
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.small
                                    color: Theme.ink2
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: "Check that fingerprint with them before you accept. "
                                          + "Accepting installs this project's key on this computer."
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.small
                                    color: Theme.amber
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                                PrimaryButton {
                                    text: "Accept and join"
                                    accessibleName: "Accept the invitation to "
                                                    + (modelData.projectName || "this project")
                                    onClicked: screen.acceptInvite(modelData.inviteId)
                                }
                            }
                        }
                    }
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
