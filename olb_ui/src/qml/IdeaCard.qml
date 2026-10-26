import QtQuick
import QtQuick.Layouts

// One idea on the board (docs/03 S1 step 4 + "Idea card data").
Rectangle {
    id: card
    property var idea: ({})
    property int stageCount: 6
    signal titleClicked(string openId)
    signal commitStripClicked(string ideaId)

    // "Stage {n} of {N} · {Stage}", or "Published · all {N} stages".
    readonly property string stageText:
        idea.published ? "Published · all " + stageCount + " stages"
                       : "Stage " + idea.stage + " of " + stageCount + " · " + stageName
    property string stageName: ""
    // violet normally; slate when stage 1 and author-only; accent when published.
    readonly property string stageKind:
        idea.published ? "done"
      : (idea.stage === 1 && String(idea.privacy).indexOf("Only you") === 0) ? "neutral"
      : "current"

    color: Theme.surface
    radius: Theme.rIdeaCard
    border.width: 1
    border.color: Theme.line
    implicitHeight: col.implicitHeight + Theme.s20 * 2

    ColumnLayout {
        id: col
        anchors.fill: parent
        anchors.margins: Theme.s20
        spacing: Theme.s12

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s8
            Badge { text: card.stageText; kind: card.stageKind }
            Item { Layout.fillWidth: true }
            Text {
                text: card.idea.updated || ""
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
            }
        }

        Text {
            text: card.idea.title || ""
            font.family: Theme.sans
            font.pixelSize: Theme.cardTitle
            font.weight: Theme.semibold
            color: Theme.ink
            wrapMode: Text.Wrap
            Layout.fillWidth: true

            Accessible.role: Accessible.Link
            Accessible.name: card.idea.title || ""
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: card.titleClicked(card.idea.open || "")
            }
        }

        Text {
            text: card.idea.summary || ""
            font.family: Theme.sans
            font.pixelSize: Theme.body
            lineHeight: Theme.bodyLineHeight
            lineHeightMode: Text.ProportionalHeight
            color: Theme.ink2
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        // The four computed facts, two per row.
        GridLayout {
            columns: 2
            columnSpacing: Theme.s20
            rowSpacing: Theme.s6
            Layout.fillWidth: true
            Repeater {
                model: card.idea.facts || []
                delegate: RowLayout {
                    required property var modelData
                    spacing: Theme.s6
                    Text {
                        text: modelData.k
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        color: Theme.ink3
                    }
                    Text {
                        text: modelData.v
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        font.weight: Theme.medium
                        color: Theme.ink2
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }
            }
        }

        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.lineSoft }

        // Commit strip — opens S6 filtered to this idea. Hashes are violet.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s8

            Text {
                text: card.idea.branch || ""
                font.family: Theme.mono
                font.pixelSize: Theme.meta
                color: Theme.violet
            }
            Text {
                text: card.idea.headCommit || ""
                font.family: Theme.mono
                font.pixelSize: Theme.meta
                color: Theme.violet
            }
            Text {
                text: card.idea.headMessage || ""
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
            Text {
                text: card.idea.commitSummary || ""
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
            }

        }

        // Click target for the commit strip. A MouseArea cannot use
        // anchors.fill inside a layout-managed item, so it is a sibling
        // sized by the layout instead.
        MouseArea {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            cursorShape: Qt.PointingHandCursor
            onClicked: card.commitStripClicked(card.idea.id || "")
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s8
            Text {
                text: (card.idea.lead || "") + " · " + (card.idea.people || "")
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
            }
            Item { Layout.fillWidth: true }
            Text {
                text: card.idea.privacy || ""
                font.family: Theme.sans
                font.pixelSize: Theme.meta
                color: Theme.ink3
            }
        }
    }
}
