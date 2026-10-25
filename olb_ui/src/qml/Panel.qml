import QtQuick
import QtQuick.Layouts

// A titled card. Children are laid out in a column inside the card body, so
// callers just declare their content.
Rectangle {
    id: panel
    property string title: ""
    property string subtitle: ""
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

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s8
            Text {
                text: panel.title
                font.family: Theme.sans
                font.pixelSize: Theme.sectionTitle
                font.weight: Theme.semibold
                color: Theme.ink
            }
            Item { Layout.fillWidth: true }
            Text {
                text: panel.subtitle
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
            }
        }

        ColumnLayout {
            id: body
            Layout.fillWidth: true
            spacing: Theme.s12
        }
    }
}
