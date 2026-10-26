import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S1 · Project board (docs/03, design/canvas/Main.dc.html).
Item {
    id: board
    property var data: ({})
    property bool fixtures: false
    signal openRecord(string recordId)
    signal openHistory(string ideaId)

    readonly property var ideas: data.ideas || []
    readonly property var stages: data.stagesUsed || []
    property string filter: "All"

    // Counts per stage: an idea counts at its CURRENT stage, and a published
    // idea counts under Publish regardless of stage (docs/03 S1 step 3).
    function stageOf(idea) {
        if (idea.published) return "Publish";
        var i = idea.stage - 1;
        return (i >= 0 && i < stages.length) ? stages[i] : "";
    }
    readonly property var pillEntries: {
        var out = [{ label: "All", count: ideas.length }];
        for (var s = 0; s < stages.length; s++) {
            var n = 0;
            for (var i = 0; i < ideas.length; i++)
                if (stageOf(ideas[i]) === stages[s]) n++;
            out.push({ label: stages[s], count: n });
        }
        return out;
    }
    readonly property var visibleIdeas: {
        if (filter === "All") return ideas;
        var out = [];
        for (var i = 0; i < ideas.length; i++)
            if (stageOf(ideas[i]) === filter) out.push(ideas[i]);
        return out;
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: board.width
            spacing: Theme.s24

            // ---- 1. PageHeader ----
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.topMargin: Theme.s32
                spacing: Theme.s12

                Text {
                    text: "Project"
                    font.family: Theme.sans
                    font.pixelSize: Theme.overline
                    font.weight: Theme.medium
                    font.letterSpacing: Theme.overlineSpacing
                    color: Theme.ink3
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s16

                    Text {
                        text: board.data.title || ""
                        font.family: Theme.sans
                        font.pixelSize: Theme.pageTitle
                        font.weight: Theme.semibold
                        font.letterSpacing: Theme.titleSpacing
                        color: Theme.ink
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }

                    Button {
                        text: "New notebook entry"
                        implicitHeight: Theme.touchTarget
                        Accessible.name: "New notebook entry"
                    }
                    Button {
                        text: "New project"
                        implicitHeight: Theme.touchTarget
                        highlighted: true
                        Accessible.name: "New project"
                    }
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: Theme.s8
                    Repeater {
                        model: board.data.badges || []
                        delegate: Badge {
                            required property var modelData
                            text: modelData.text
                            kind: modelData.kind
                        }
                    }
                    Badge {
                        visible: board.fixtures
                        text: "Fixture data"
                        kind: "attention"
                    }
                }
            }

            // ---- 2. Status cards ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                columns: board.width > 1100 ? 3 : (board.width > 700 ? 2 : 1)
                columnSpacing: Theme.s16
                rowSpacing: Theme.s16

                StatusCard {
                    Layout.fillWidth: true
                    label: (board.data.status && board.data.status.sync) ? board.data.status.sync.label : ""
                    value: (board.data.status && board.data.status.sync) ? board.data.status.sync.value : ""
                    sub:   (board.data.status && board.data.status.sync) ? board.data.status.sync.sub : ""
                }
                StatusCard {
                    Layout.fillWidth: true
                    readonly property var c: (board.data.status && board.data.status.copies) ? board.data.status.copies : ({})
                    label: c.label || ""
                    value: c.value || ""
                    warning: c.warning || ""
                    actionText: c.warningAction ? c.warningAction.text : ""
                    onActionClicked: if (c.warningAction) board.openRecord(c.warningAction.recordId)
                }
                StatusCard {
                    Layout.fillWidth: true
                    readonly property var h: (board.data.status && board.data.status.history) ? board.data.status.history : ({})
                    label: h.label || ""
                    value: h.value || ""
                    sub: h.sub || ""
                    actionText: "Open history"
                    onActionClicked: board.openHistory("")
                }
            }

            // ---- 3. Ideas heading + filters ----
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                spacing: Theme.s12

                Text {
                    text: "Ideas"
                    font.family: Theme.sans
                    font.pixelSize: Theme.cardTitle
                    font.weight: Theme.semibold
                    color: Theme.ink
                }

                FilterPills {
                    Layout.fillWidth: true
                    entries: board.pillEntries
                    selected: board.filter
                    onPicked: function (label) { board.filter = label; }
                }
            }

            // ---- 4. Idea grid, auto-fill, min 460px ----
            GridLayout {
                id: grid
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                columns: Math.max(1, Math.floor((board.width - Theme.s32 * 2 + Theme.s16) / (460 + Theme.s16)))
                columnSpacing: Theme.s16
                rowSpacing: Theme.s16
                visible: board.visibleIdeas.length > 0

                Repeater {
                    model: board.visibleIdeas
                    delegate: IdeaCard {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignTop
                        idea: modelData
                        stageCount: board.stages.length
                        stageName: board.stageOf(modelData)
                        onTitleClicked: function (openId) { board.openRecord(openId); }
                        onCommitStripClicked: function (ideaId) { board.openHistory(ideaId); }
                    }
                }
            }

            // ---- 5. Empty state ----
            Rectangle {
                visible: board.visibleIdeas.length === 0
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                implicitHeight: 120
                radius: Theme.rPanel
                color: "transparent"
                border.width: 1
                border.color: Theme.dashed
                Text {
                    anchors.centerIn: parent
                    text: "No ideas are at this stage right now."
                    font.family: Theme.sans
                    font.pixelSize: Theme.body
                    color: Theme.ink3
                }
            }

            // ---- 6. Activity ----
            Panel {
                title: "Activity"
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s32

                Repeater {
                    model: board.data.activity || []
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: Theme.s16
                        Text {
                            text: modelData.text
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            color: Theme.ink2
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                            MouseArea {
                                anchors.fill: parent
                                enabled: !!modelData.link
                                cursorShape: Qt.PointingHandCursor
                                onClicked: board.openRecord(modelData.link)
                            }
                        }
                        Text {
                            text: modelData.time || ""
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            color: Theme.ink3
                        }
                    }
                }
            }
        }
    }
}
