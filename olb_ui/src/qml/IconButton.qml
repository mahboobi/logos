import QtQuick
import QtQuick.Controls
import QtQuick.Shapes

// A square, icon-only button.
//
// Same focusable-Rectangle construction as PrimaryButton, and for the same
// reason: the native macOS style refuses to let a Button's background and
// contentItem be customised, so anything built on Button paints itself in
// system colours the design system never chose.
//
// An icon with no label has to earn its place on three counts (docs/02 §6):
// it keeps the full 44 px touch target, it carries an accessible name, and it
// shows a tooltip on hover and on keyboard focus — so the meaning is
// reachable without a pointer.
Rectangle {
    id: button

    // Path data and the viewBox edge it was authored against, e.g.
    // `pathData: Icons.refresh; viewBox: Icons.refreshViewBox`.
    property string pathData: ""
    property int viewBox: 30
    property int iconSize: 18

    property bool enabled: true
    property string accessibleName: ""
    property string tooltip: ""
    signal clicked()

    readonly property color fg: !enabled ? Theme.ink3
                                         : (hover.hovered ? Theme.ink : Theme.ink2)

    implicitHeight: Theme.touchTarget
    implicitWidth: Theme.touchTarget
    radius: Theme.rControl
    color: !enabled ? Theme.surfaceSunken
                    : (hover.hovered ? Theme.surfaceSunken : Theme.surface)
    border.width: 1
    border.color: enabled ? Theme.inputBorder : Theme.lineSoft

    Accessible.role: Accessible.Button
    Accessible.name: accessibleName
    Accessible.description: tooltip
    Accessible.onPressAction: if (button.enabled) button.clicked()
    activeFocusOnTab: enabled

    Rectangle {
        anchors.fill: parent
        anchors.margins: -3
        radius: Theme.rControl
        color: "transparent"
        border.width: 2
        border.color: Theme.violet
        visible: parent.activeFocus
    }

    Item {
        anchors.centerIn: parent
        width: button.iconSize
        height: button.iconSize

        Shape {
            width: button.viewBox
            height: button.viewBox
            preferredRendererType: Shape.CurveRenderer
            transform: Scale {
                xScale: button.iconSize / button.viewBox
                yScale: button.iconSize / button.viewBox
            }

            ShapePath {
                fillColor: button.fg
                fillRule: ShapePath.WindingFill
                strokeWidth: 0
                strokeColor: "transparent"
                PathSvg { path: button.pathData }
            }
        }
    }

    ToolTip {
        text: button.tooltip
        delay: 400
        visible: button.tooltip !== "" && (hover.hovered || button.activeFocus)
    }

    HoverHandler { id: hover; enabled: button.enabled }

    MouseArea {
        anchors.fill: parent
        enabled: button.enabled
        cursorShape: Qt.PointingHandCursor
        onClicked: button.clicked()
    }
    Keys.onReturnPressed: if (button.enabled) button.clicked()
    Keys.onSpacePressed: if (button.enabled) button.clicked()
}
