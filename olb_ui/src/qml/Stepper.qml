import QtQuick
import QtQuick.Layouts

// The four pre-registration steps on S3 (docs/02 §4 `Stepper`): a done step
// carries an accent numbered circle, the current step a violet 2 px card, a
// later step a sunken circle. The cards reflow onto fewer columns as the
// window narrows.
GridLayout {
    id: stepper

    // [{ title, note, state: "done" | "current" | "later" }]
    property var steps: []

    readonly property int minCardWidth: 200
    columns: Math.max(1, Math.min(stepper.steps.length,
                                  Math.floor(stepper.width / stepper.minCardWidth)))
    columnSpacing: Theme.s8
    rowSpacing: Theme.s8

    Accessible.role: Accessible.List
    Accessible.name: "Pre-registration steps"

    Repeater {
        model: stepper.steps

        delegate: Rectangle {
            id: card
            required property var modelData
            required property int index
            readonly property bool isCurrent: modelData.state === "current"
            readonly property bool isDone: modelData.state === "done"

            Layout.fillWidth: true
            Layout.fillHeight: true
            implicitHeight: Math.max(Theme.touchTarget, cardRow.implicitHeight + Theme.s12 * 2)
            radius: Theme.rInner
            color: Theme.surface
            border.width: card.isCurrent ? 2 : 1
            border.color: card.isCurrent ? Theme.violet : Theme.line

            Accessible.role: Accessible.StaticText
            Accessible.name: (card.index + 1) + ". " + (modelData.title || "")
                             + ", " + (modelData.note || "")
                             + (card.isCurrent ? ", current step" : "")

            RowLayout {
                id: cardRow
                anchors.fill: parent
                // 2 px borders eat a pixel of padding so the card keeps its size.
                anchors.margins: card.isCurrent ? Theme.s12 - 1 : Theme.s12
                anchors.leftMargin: card.isCurrent ? Theme.s14 - 1 : Theme.s14
                anchors.rightMargin: card.isCurrent ? Theme.s14 - 1 : Theme.s14
                spacing: Theme.s10

                Rectangle {
                    Layout.alignment: Qt.AlignVCenter
                    width: Theme.s24
                    height: Theme.s24
                    radius: width / 2
                    color: card.isDone    ? Theme.accent
                         : card.isCurrent ? Theme.violet
                                          : Theme.surfaceSunken

                    Text {
                        anchors.centerIn: parent
                        text: card.index + 1
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        font.weight: Theme.semibold
                        color: card.isDone || card.isCurrent ? Theme.textOnAccent : Theme.ink2
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    Text {
                        text: modelData.title || ""
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        font.weight: card.isCurrent ? Theme.semibold : Theme.medium
                        color: Theme.ink
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Text {
                        visible: (modelData.note || "") !== ""
                        text: modelData.note || ""
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        color: card.isDone    ? Theme.accent
                             : card.isCurrent ? Theme.violet
                                              : Theme.ink3
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
