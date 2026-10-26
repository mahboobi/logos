import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Shell: left nav + the active screen. docs/03 names the nav entries; only
// Board is implemented so far, so the rest are visibly disabled rather than
// pretending to work.
Rectangle {
    id: root
    color: Theme.ground

    readonly property var backend: logos.module("olb_ui")
    property bool ready: false
    property string screen: "Board"

    readonly property var boardData: {
        try { return JSON.parse(backend ? backend.board : "{}"); } catch (e) { return ({}); }
    }
    readonly property bool fixtures: backend ? backend.fixtures : false

    // docs/03 nav. `done` marks what actually exists today.
    readonly property var navItems: [
        { label: "Board",       done: true  },
        { label: "Notebook",    done: false },
        { label: "Hypotheses",  done: false },
        { label: "Funding",     done: false },
        { label: "Publish",     done: false },
        { label: "History",     done: false },
        { label: "Members",     done: false },
        { label: "Diagnostics", done: true  }
    ]

    Connections {
        target: logos
        function onViewModuleReadyChanged(moduleName, isReady) {
            if (moduleName === "olb_ui") {
                root.ready = isReady && root.backend !== null;
                if (root.ready) root.backend.refreshBoard();
            }
        }
    }
    Component.onCompleted: {
        root.ready = root.backend !== null && logos.isViewModuleReady("olb_ui");
        if (root.ready) root.backend.refreshBoard();
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // ---- Left nav ----
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 200
            color: Theme.surface
            border.width: 0

            Rectangle {   // right hairline
                anchors.right: parent.right
                width: 1; height: parent.height
                color: Theme.line
            }

            ColumnLayout {
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.margins: Theme.s12
                spacing: Theme.s2

                Text {
                    text: "Open Labbook"
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    font.weight: Theme.semibold
                    color: Theme.ink
                    Layout.margins: Theme.s8
                }

                Repeater {
                    model: root.navItems
                    delegate: Rectangle {
                        required property var modelData
                        readonly property bool active: root.screen === modelData.label
                        Layout.fillWidth: true
                        implicitHeight: Theme.touchTarget
                        radius: Theme.rControl
                        color: active ? Theme.accentTint : "transparent"

                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.label
                                         + (modelData.done ? "" : ", not built yet")
                        activeFocusOnTab: modelData.done

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.left: parent.left
                            anchors.leftMargin: Theme.s12
                            text: modelData.label
                            font.family: Theme.sans
                            font.pixelSize: Theme.body
                            font.weight: parent.active ? Theme.medium : Theme.regular
                            color: !modelData.done ? Theme.dashed
                                                   : (parent.active ? Theme.accent : Theme.ink2)
                        }

                        MouseArea {
                            anchors.fill: parent
                            enabled: modelData.done
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: root.screen = modelData.label
                        }
                        Keys.onReturnPressed: if (modelData.done) root.screen = modelData.label
                    }
                }
            }
        }

        // ---- Active screen ----
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.screen === "Board" ? 0 : 1

            Board {
                boardData: root.boardData
                fixtures: root.fixtures
                onOpenRecord: function (recordId) {
                    // S2 is not built yet; surface the intent rather than
                    // navigating nowhere.
                    console.log("[olb_ui] open record:", recordId);
                }
                onOpenHistory: function (ideaId) {
                    console.log("[olb_ui] open history for:", ideaId);
                }
            }

            Diagnostics {
                backend: root.backend
                ready: root.ready
            }
        }
    }
}
