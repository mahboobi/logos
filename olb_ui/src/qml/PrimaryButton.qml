import QtQuick

// A themed button.
//
// Qt's native macOS style REFUSES customisation of Button.background and
// contentItem ("The current style does not support customization of this
// control"), so `Button { highlighted: true }` paints system blue — a colour
// the design system never chose. This is the focusable-Rectangle pattern
// FilterPills, ChoiceCard and History already use, which the native style
// does not touch.
Rectangle {
    id: button
    property string text: ""
    property bool primary: true
    property bool enabled: true
    property string accessibleName: ""
    signal clicked()

    readonly property color fg: !enabled ? Theme.ink3
                              : primary  ? Theme.textOnAccent
                                         : Theme.ink
    readonly property color bg: !enabled ? Theme.surfaceSunken
                              : primary  ? (hover.hovered ? Theme.accentHover : Theme.accent)
                                         : Theme.surface

    implicitHeight: Theme.touchTarget
    implicitWidth: label.implicitWidth + Theme.s20 * 2
    radius: Theme.rControl
    color: bg
    border.width: primary ? 0 : 1
    border.color: enabled ? Theme.inputBorder : Theme.lineSoft

    Accessible.role: Accessible.Button
    Accessible.name: accessibleName !== "" ? accessibleName : text
    Accessible.onPressAction: if (button.enabled) button.clicked()
    activeFocusOnTab: enabled

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
        id: label
        anchors.centerIn: parent
        text: button.text
        font.family: Theme.sans
        font.pixelSize: Theme.body
        font.weight: Theme.medium
        color: button.fg
    }

    HoverHandler { id: hover; enabled: button.enabled }

    MouseArea {
        anchors.fill: parent
        enabled: button.enabled
        cursorShape: Qt.PointingHandCursor
        onClicked: button.clicked()
    }
    Keys.onReturnPressed: if (button.enabled) button.clicked()
    Keys.onSpacePressed: if (button.enabled) button.clicked()
}
