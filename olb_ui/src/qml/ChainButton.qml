import QtQuick

// The `violetPrimary` button from docs/02 §4: violet fill, white label, and
// reserved for actions that write to the chain — "Commit to LEZ" on S3 and
// merge requests. Violet means committed here, not decoration.
//
// Built as a focusable Rectangle for the reason PrimaryButton documents: the
// native macOS style refuses to let a Button's background be customised and
// paints system blue instead.
Rectangle {
    id: button
    property string text: ""
    property string accessibleName: ""
    property string disabledReason: ""
    signal clicked()

    implicitHeight: Theme.touchTarget
    implicitWidth: caption.implicitWidth + Theme.s20 * 2
    radius: Theme.rControl
    color: !button.enabled ? Theme.surfaceSunken
                           : (hover.hovered ? Qt.darker(Theme.violet, 1.2) : Theme.violet)
    border.width: button.enabled ? 0 : 1
    border.color: Theme.lineSoft

    Accessible.role: Accessible.Button
    Accessible.name: accessibleName !== "" ? accessibleName : text
    Accessible.description: button.enabled ? "" : button.disabledReason
    Accessible.onPressAction: if (button.enabled) button.clicked()
    activeFocusOnTab: button.enabled

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
        id: caption
        anchors.centerIn: parent
        text: button.text
        font.family: Theme.sans
        font.pixelSize: Theme.body
        font.weight: Theme.semibold
        color: button.enabled ? Theme.textOnAccent : Theme.ink3
    }

    HoverHandler { id: hover }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: button.clicked()
    }
    Keys.onReturnPressed: if (button.enabled) button.clicked()
    Keys.onSpacePressed: if (button.enabled) button.clicked()
}
