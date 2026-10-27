import QtQuick

// A text link that is reachable by keyboard and, standing on its own, keeps
// the 44 px target (docs/02 §3). `compact` drops to the text's own height for
// links that sit inside a sentence or a row of chips, where the row already
// provides the target.
Item {
    id: link
    property string label: ""
    property color fg: Theme.accent
    property int size: Theme.body
    property int weight: Theme.medium
    property bool compact: false
    property string accessibleName: ""
    signal clicked()

    implicitWidth: caption.implicitWidth
    implicitHeight: link.compact ? caption.implicitHeight : Theme.touchTarget

    Accessible.role: Accessible.Link
    Accessible.name: link.accessibleName !== "" ? link.accessibleName : link.label
    Accessible.onPressAction: link.clicked()
    activeFocusOnTab: true

    Rectangle {               // focus ring: 2 px, 2 px offset (docs/02 §6)
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
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        text: link.label
        font.family: Theme.sans
        font.pixelSize: link.size
        font.weight: link.weight
        font.underline: hover.hovered
        color: hover.hovered ? Qt.darker(link.fg, 1.3) : link.fg
    }

    HoverHandler { id: hover }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: link.clicked()
    }
    Keys.onReturnPressed: link.clicked()
    Keys.onSpacePressed: link.clicked()
}
