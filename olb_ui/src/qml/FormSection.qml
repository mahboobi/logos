import QtQuick
import QtQuick.Layouts

// A numbered form section: "1 · Basics".
Rectangle {
    id: sec
    property int index: 1
    property string title: ""
    property string hint: ""
    default property alias content: body.data

    color: Theme.surface
    radius: Theme.rPanel
    border.width: 1
    border.color: Theme.line
    implicitHeight: layout.implicitHeight + Theme.s20 * 2

    ColumnLayout {
        id: layout
        anchors.fill: parent
        anchors.margins: Theme.s20
        spacing: Theme.s12

        Text {
            text: sec.index + " · " + sec.title
            font.family: Theme.sans
            font.pixelSize: Theme.sectionTitle
            font.weight: Theme.semibold
            color: Theme.ink
        }
        Text {
            visible: sec.hint !== ""
            text: sec.hint
            font.family: Theme.sans
            font.pixelSize: Theme.small
            color: Theme.ink3
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        ColumnLayout {
            id: body
            Layout.fillWidth: true
            spacing: Theme.s12
        }
    }
}
