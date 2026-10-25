import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Phase 0 task 3 skeleton. Deliberately unstyled: design/Theme.qml tokens and
// the real screens land in Phase 1a (docs/02, docs/03). Nothing here should
// survive into a shipped screen.
Item {
    id: root

    // Typed replica — auto-synced PROPs and callable SLOTs from olb_ui.rep.
    readonly property var backend: logos.module("olb_ui")
    property bool ready: false

    readonly property string coreStatus: backend ? backend.coreStatus : ""
    readonly property bool coreReachable: backend ? backend.coreReachable : false
    readonly property string identity: backend ? backend.identity : ""
    readonly property string recordsJson: backend ? backend.records : "[]"

    // olb_core hands records over as a JSON string (docs/08 R10).
    readonly property var recordModel: {
        try { return JSON.parse(root.recordsJson); } catch (e) { return []; }
    }
    readonly property int recordCount: root.recordModel.length

    Connections {
        target: logos
        function onViewModuleReadyChanged(moduleName, isReady) {
            if (moduleName === "olb_ui")
                root.ready = isReady && root.backend !== null;
        }
    }

    Component.onCompleted: {
        root.ready = root.backend !== null && logos.isViewModuleReady("olb_ui");
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        // No explicit colour: the template's #ffffff was invisible on
        // Basecamp's light background. Phase 1a replaces this with a
        // design/Theme.qml token (docs/02).
        Text {
            text: "Open Labbook — Phase 0 skeleton"
            font.pixelSize: 20
            Layout.alignment: Qt.AlignHCenter
        }

        Text {
            text: root.ready ? "olb_ui backend connected"
                             : "Connecting to olb_ui backend..."
            color: root.ready ? "#56d364" : "#f0883e"
            font.pixelSize: 12
        }

        Button {
            text: "Refresh session status"
            enabled: root.ready
            onClicked: root.backend.refreshSessionStatus()
        }

        Text {
            text: root.coreReachable ? "olb_core reachable" : "olb_core not reached"
            color: root.coreReachable ? "#56d364" : "#f0883e"
            font.pixelSize: 12
        }

        // The JSON string olb_core.session_status() returns, pushed into the
        // coreStatus PROP by the backend and synced here over QtRO.
        Text {
            text: root.coreStatus === "" ? "No status yet" : root.coreStatus
            color: "#8b949e"
            font.pixelSize: 13
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }

        // ---- Phase 0 task 4: signed records over Delivery ----
        Rectangle { Layout.fillWidth: true; height: 1; color: "#d0d7de" }

        Text {
            text: "Identity (ed25519): " + (root.identity === "" ? "—" : root.identity)
            font.pixelSize: 11
            color: "#8b949e"
            Layout.fillWidth: true
            elide: Text.ElideMiddle
        }

        RowLayout {
            spacing: 12
            Layout.fillWidth: true

            TextField {
                id: recordTitle
                placeholderText: "Record title"
                Layout.fillWidth: true
            }

            Button {
                text: "Publish signed record"
                enabled: root.ready && recordTitle.text.length > 0
                onClicked: {
                    root.backend.publishTestRecord(recordTitle.text);
                    recordTitle.text = "";
                }
            }
        }

        Text {
            text: "Records received (" + root.recordCount + ")"
            font.pixelSize: 13
        }

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.recordModel
            delegate: Text {
                width: ListView.view.width
                font.pixelSize: 12
                color: "#8b949e"
                elide: Text.ElideRight
                text: "• " + modelData.payload.title
                      + "   by " + String(modelData.author).substring(0, 16) + "…"
            }
        }
    }
}
