import QtQuick
import QtQuick.Layouts

// docs/02 §4 `ProvenanceChain`: idea → hypothesis → funding → experiments →
// data → manuscript, each node carrying a stage overline, a title and a status
// in a semantic colour. The last node is the one being published, so it is
// highlighted accent. Nodes wrap onto more rows as the window narrows; the
// grid keeps every card in a row the same height.
GridLayout {
    id: chain

    // [{ stage, title, status, kind: "done"|"current"|"neutral"|"attention"|"final" }]
    property var nodes: []

    readonly property int minCardWidth: 190
    readonly property int arrowWidth: 18

    columns: Math.max(1, Math.min(chain.nodes.length,
                                  Math.floor(chain.width / chain.minCardWidth)))
    columnSpacing: Theme.s8
    rowSpacing: Theme.s8

    Accessible.role: Accessible.List
    Accessible.name: "Provenance chain"

    Repeater {
        model: chain.nodes

        delegate: RowLayout {
            id: cell
            required property var modelData
            required property int index
            readonly property bool isFinal: modelData.kind === "final"

            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Theme.s8

            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                implicitHeight: nodeCol.implicitHeight + Theme.s12 * 2
                radius: Theme.rInner
                color: cell.isFinal ? Theme.accentTint : Theme.surface
                border.width: cell.isFinal ? 2 : 1
                border.color: cell.isFinal ? Theme.accent : Theme.line

                Accessible.role: Accessible.StaticText
                Accessible.name: (modelData.stage || "") + ": " + (modelData.title || "")
                                 + ", " + (modelData.status || "")

                ColumnLayout {
                    id: nodeCol
                    anchors.fill: parent
                    anchors.margins: cell.isFinal ? Theme.s12 - 1 : Theme.s12
                    spacing: Theme.s4

                    Text {
                        text: (modelData.stage || "").toUpperCase()
                        font.family: Theme.sans
                        font.pixelSize: Theme.overline
                        font.weight: Theme.semibold
                        font.letterSpacing: Theme.overlineSpacing
                        color: cell.isFinal ? Theme.accent : Theme.ink3
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                    }
                    Text {
                        text: modelData.title || ""
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        font.weight: Theme.medium
                        color: Theme.ink
                        lineHeight: 1.35
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Item { Layout.fillHeight: true; Layout.minimumHeight: 0 }
                    Text {
                        text: modelData.status || ""
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        color: modelData.kind === "done" || cell.isFinal ? Theme.accent
                             : modelData.kind === "current"              ? Theme.violet
                             : modelData.kind === "attention"            ? Theme.amber
                                                                         : Theme.slate
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
            }

            // The arrow keeps its space on the last node so every cell in a
            // row is the same width.
            Text {
                Layout.preferredWidth: chain.arrowWidth
                Layout.alignment: Qt.AlignVCenter
                horizontalAlignment: Text.AlignHCenter
                text: "→"
                font.family: Theme.sans
                font.pixelSize: Theme.cardTitle
                color: Theme.dashed
                opacity: cell.index < chain.nodes.length - 1 ? 1 : 0
                Accessible.ignored: true
            }
        }
    }
}
