import QtQuick
import QtQuick.Layouts

// A selectable card used for stages (checkbox) and privacy/anchoring (radio).
Rectangle {
    id: card
    property string title: ""
    property string body: ""
    property string note: ""
    property bool checked: false
    property bool radio: false
    signal toggled()

    radius: Theme.rInner
    border.width: 1
    border.color: checked ? Theme.accent : Theme.line
    color: checked ? Theme.accentTintSoft : Theme.surface
    implicitHeight: row.implicitHeight + Theme.s16 * 2

    Accessible.role: card.radio ? Accessible.RadioButton : Accessible.CheckBox
    Accessible.name: card.title
    Accessible.checked: card.checked
    activeFocusOnTab: true

    Rectangle {
        anchors.fill: parent
        anchors.margins: -3
        radius: Theme.rInner
        color: "transparent"
        border.width: 2
        border.color: Theme.violet
        visible: parent.activeFocus
    }

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: Theme.s16
        spacing: Theme.s12

        // Indicator: square for stages, circle for exclusive choices.
        Rectangle {
            Layout.alignment: Qt.AlignTop
            width: 18; height: 18
            radius: card.radio ? 9 : 4
            color: card.checked && !card.radio ? Theme.accent : Theme.surface
            border.width: card.checked ? 2 : 1
            border.color: card.checked ? Theme.accent : Theme.inputBorder

            Text {
                visible: card.checked && !card.radio
                anchors.centerIn: parent
                text: "✓"
                color: Theme.textOnAccent
                font.pixelSize: 12
                font.weight: Theme.semibold
            }
            Rectangle {
                visible: card.checked && card.radio
                anchors.centerIn: parent
                width: 8; height: 8; radius: 4
                color: Theme.accent
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.s4
            Text {
                text: card.title
                font.family: Theme.sans
                font.pixelSize: Theme.small
                font.weight: Theme.semibold
                color: Theme.ink
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Text {
                visible: card.body !== ""
                text: card.body
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink2
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Text {
                visible: card.note !== ""
                text: card.note
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: card.toggled()
    }
    Keys.onReturnPressed: card.toggled()
    Keys.onSpacePressed: card.toggled()
}
