import QtQuick

// Small pill label. `kind` follows CLAUDE.md colour semantics:
// neutral = slate, done = accent (verified/published), attention = amber,
// current = violet.
Rectangle {
    id: badge
    property string text: ""
    property string kind: "neutral"

    readonly property color fg: kind === "done"      ? Theme.accent
                              : kind === "attention" ? Theme.amber
                              : kind === "current"   ? Theme.violet
                                                     : Theme.slate
    readonly property color bg: kind === "done"      ? Theme.accentTint
                              : kind === "attention" ? Theme.amberTint
                              : kind === "current"   ? Theme.violetTint
                                                     : Theme.slateTint

    implicitHeight: Theme.chipHeight
    implicitWidth: label.implicitWidth + Theme.s12 * 2
    radius: Theme.rPill
    color: bg

    Text {
        id: label
        anchors.centerIn: parent
        text: badge.text
        font.family: Theme.sans
        font.pixelSize: Theme.meta
        font.weight: Theme.medium
        color: badge.fg
    }
}
