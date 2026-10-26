import QtQuick
import QtQuick.Layouts

// One of the three board status cards (docs/03 S1 step 2).
Rectangle {
    id: card
    property string label: ""
    property string value: ""
    property string sub: ""
    property string warning: ""
    property string actionText: ""
    signal actionClicked()

    color: Theme.surface
    radius: Theme.rPanel
    border.width: 1
    border.color: Theme.line
    implicitHeight: col.implicitHeight + Theme.s20 * 2

    ColumnLayout {
        id: col
        anchors.fill: parent
        anchors.margins: Theme.s20
        spacing: Theme.s6

        Text {
            text: card.label
            font.family: Theme.sans
            font.pixelSize: Theme.overline
            font.weight: Theme.medium
            font.letterSpacing: Theme.overlineSpacing
            color: Theme.ink3
            Layout.fillWidth: true
        }
        Text {
            text: card.value
            font.family: Theme.sans
            font.pixelSize: Theme.metric
            font.weight: Theme.semibold
            color: Theme.ink
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        Text {
            visible: card.sub !== ""
            text: card.sub
            font.family: Theme.sans
            font.pixelSize: Theme.small
            color: Theme.ink3
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }

        // Amber only when something actually needs attention.
        RowLayout {
            visible: card.warning !== ""
            Layout.fillWidth: true
            spacing: Theme.s6
            Text {
                text: card.warning
                font.family: Theme.sans
                font.pixelSize: Theme.small
                color: Theme.amber
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }
        Text {
            visible: card.actionText !== ""
            text: card.actionText
            font.family: Theme.sans
            font.pixelSize: Theme.small
            font.underline: true
            color: Theme.amber
            Layout.fillWidth: true

            Accessible.role: Accessible.Link
            Accessible.name: card.actionText
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: card.actionClicked()
            }
        }
    }
}
