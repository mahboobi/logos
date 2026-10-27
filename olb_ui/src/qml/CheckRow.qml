import QtQuick
import QtQuick.Layouts

// A checkbox row with a title, an optional status note and an optional action
// link — the "What to reveal" rows on S5 and the two acknowledgements on S3.
//
// Not a QtQuick.Controls CheckBox: the native style will not take the accent
// indicator colour the design system asks for, and the row needs a second
// line whose colour carries meaning (`noteKind`).
Item {
    id: row
    property string title: ""
    property string note: ""
    // done = verified, attention = needs action, current = committed,
    // neutral = plain meta text (docs/02 §1).
    property string noteKind: "neutral"
    property bool checked: false
    property color indicatorColor: Theme.accent
    property string actionText: ""
    property bool divider: false
    property int pad: Theme.s12
    signal toggled()
    signal actionClicked()

    readonly property color noteColor: noteKind === "done"      ? Theme.accent
                                     : noteKind === "attention" ? Theme.amber
                                     : noteKind === "current"   ? Theme.violet
                                                                : Theme.ink3

    implicitHeight: Math.max(Theme.touchTarget, content.implicitHeight + row.pad * 2)

    Accessible.role: Accessible.CheckBox
    Accessible.name: row.note === "" ? row.title : row.title + ". " + row.note
    Accessible.checked: row.checked
    Accessible.onToggleAction: row.toggled()
    Accessible.onPressAction: row.toggled()
    activeFocusOnTab: true

    Rectangle {
        anchors.fill: parent
        anchors.margins: -1
        radius: Theme.rControl
        color: "transparent"
        border.width: 2
        border.color: Theme.violet
        visible: parent.activeFocus
    }

    // Declared before the content so the action link, which has its own
    // handler, stays on top of this one.
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: row.toggled()
    }

    RowLayout {
        id: content
        anchors.fill: parent
        anchors.margins: row.pad
        spacing: Theme.s12

        Rectangle {
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: Theme.s2
            width: Theme.checkbox
            height: Theme.checkbox
            radius: Theme.s4
            color: row.checked ? row.indicatorColor : Theme.surface
            border.width: row.checked ? 2 : 1
            border.color: row.checked ? row.indicatorColor : Theme.inputBorder

            Text {
                visible: row.checked
                anchors.centerIn: parent
                text: "✓"
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                font.weight: Theme.semibold
                color: Theme.textOnAccent
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.s2

            Text {
                text: row.title
                font.family: Theme.sans
                font.pixelSize: Theme.body
                font.weight: Theme.medium
                color: Theme.ink
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Text {
                visible: row.note !== ""
                text: row.note
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: row.noteColor
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            TextLink {
                visible: row.actionText !== ""
                label: row.actionText
                size: Theme.small
                weight: Theme.semibold
                fg: row.noteColor
                compact: true
                Layout.topMargin: Theme.s2
                onClicked: row.actionClicked()
            }
        }
    }

    Rectangle {
        visible: row.divider
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: 1
        color: Theme.lineSoft
    }

    Keys.onReturnPressed: row.toggled()
    Keys.onSpacePressed: row.toggled()
}
