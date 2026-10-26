import QtQuick
import QtQuick.Layouts

// Stage filter row (docs/03 S1 step 3). Selected pill uses an ink fill.
// Keyboard reachable: each pill is a focusable button.
Flow {
    id: pills
    property var entries: []        // [{ label, count }]
    property string selected: "All"
    signal picked(string label)

    spacing: Theme.s8

    Repeater {
        model: pills.entries
        delegate: Rectangle {
            required property var modelData
            readonly property bool active: pills.selected === modelData.label

            height: Theme.pillHeight
            width: row.implicitWidth + Theme.s16 * 2
            radius: Theme.rPill
            color: active ? Theme.ink : Theme.surface
            border.width: 1
            border.color: active ? Theme.ink : Theme.inputBorder

            Accessible.role: Accessible.Button
            Accessible.name: modelData.label + ", " + modelData.count + " ideas"
            Accessible.onPressAction: pills.picked(modelData.label)
            activeFocusOnTab: true

            Rectangle {   // visible focus ring, keyboard users need it
                anchors.fill: parent
                anchors.margins: -3
                radius: Theme.rPill
                color: "transparent"
                border.width: 2
                border.color: Theme.violet
                visible: parent.activeFocus
            }

            Row {
                id: row
                anchors.centerIn: parent
                spacing: Theme.s6
                Text {
                    text: modelData.label
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    font.weight: Theme.medium
                    color: active ? Theme.textOnAccent : Theme.ink2
                }
                Text {
                    text: modelData.count
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: active ? Theme.textOnAccent : Theme.ink3
                }
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: pills.picked(modelData.label)
            }
            Keys.onReturnPressed: pills.picked(modelData.label)
            Keys.onSpacePressed: pills.picked(modelData.label)
        }
    }
}
