import QtQuick
import QtQuick.Layouts

// The project's position in its own stages (docs/03, docs/08 D18).
//
// A stage belongs to the project, and it is only ever finished by doing its
// work — there is no control here that sets one. So this is a readout, not an
// input, and the current stage says what it is waiting for: without that,
// work-driven advancement is indistinguishable from nothing happening.
//
// Colour follows the house rule: green done, violet current, slate ahead.
Item {
    id: tracker

    /// The stage names this project uses, in order.
    property var stages: []
    /// 1-based position in `stages`.
    property int at: 1
    /// What the current stage is still waiting for, in plain words.
    property string needs: ""

    implicitHeight: col.implicitHeight

    ColumnLayout {
        id: col
        width: parent.width
        spacing: Theme.s8

        Flow {
            Layout.fillWidth: true
            spacing: Theme.s8

            Repeater {
                model: tracker.stages

                delegate: Row {
                    required property var modelData
                    required property int index
                    spacing: Theme.s8

                    readonly property int n: index + 1
                    readonly property bool done: n < tracker.at
                    readonly property bool current: n === tracker.at

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        implicitWidth: label.implicitWidth + Theme.s12 * 2
                        implicitHeight: Math.max(Theme.touchTarget - Theme.s12, 28)
                        radius: Theme.rControl
                        color: done ? Theme.accentTint
                             : current ? Theme.violetTint
                             : Theme.slateTint
                        border.width: current ? 1 : 0
                        border.color: Theme.violet

                        Accessible.role: Accessible.StaticText
                        Accessible.name: modelData + ", stage " + n + " of "
                                         + tracker.stages.length + ", "
                                         + (done ? "done" : current ? "current" : "not started")

                        Text {
                            id: label
                            anchors.centerIn: parent
                            // A tick reads as "finished" faster than a number.
                            text: (done ? "✓  " : n + ".  ") + modelData
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            font.weight: current ? Theme.semibold : Theme.regular
                            color: done ? Theme.accent
                                 : current ? Theme.violet
                                 : Theme.slate
                        }
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: index < tracker.stages.length - 1
                        text: "→"
                        font.pixelSize: Theme.small
                        color: Theme.ink3
                    }
                }
            }
        }

        // Amber: this is the thing standing between the project and its next
        // stage, which is "needs attention" rather than an error.
        Text {
            visible: tracker.needs !== ""
            Layout.fillWidth: true
            text: "To finish " + (tracker.stages[tracker.at - 1] || "this stage")
                  + ": " + tracker.needs
            font.family: Theme.sans
            font.pixelSize: Theme.small
            color: Theme.amber
            wrapMode: Text.Wrap
        }
    }
}
