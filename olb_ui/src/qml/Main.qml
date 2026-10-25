import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

// Phase 0 harness, now on design-system tokens (docs/02). This is still a
// developer surface, not a product screen: S1-S8 and M1 from docs/03 are
// Phase 1a. What it does prove is the full seam — typed calls and typed
// events across QML -> QtRO -> C++ backend -> LogosAPI -> olb_core — plus
// Theme.qml resolving as a singleton.
Rectangle {
    id: root
    color: Theme.ground

    readonly property var backend: logos.module("olb_ui")
    property bool ready: false

    readonly property string coreStatus: backend ? backend.coreStatus : ""
    readonly property bool coreReachable: backend ? backend.coreReachable : false
    readonly property string identity: backend ? backend.identity : ""
    readonly property string recordsJson: backend ? backend.records : "[]"
    readonly property string cidsJson: backend ? backend.cids : "[]"
    readonly property string lastStorageResult: backend ? backend.lastStorageResult : ""

    // olb_core hands records and CIDs over as JSON strings (docs/08 R10).
    function parseJson(text, fallback) {
        try { return JSON.parse(text); } catch (e) { return fallback; }
    }
    readonly property var recordModel: parseJson(root.recordsJson, [])
    readonly property var cidModel: parseJson(root.cidsJson, [])
    readonly property var statusObj: parseJson(root.coreStatus, ({}))

    FileDialog {
        id: uploadPicker
        title: "Choose a file to store"
        onAccepted: {
            uploadPath.text = selectedFile;
            root.backend.uploadFile(selectedFile);
        }
    }

    FolderDialog {
        id: destPicker
        title: "Choose where to save the fetched file"
        onAccepted: fetchDest.text = selectedFolder
    }

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

    // A labelled status dot. Colour carries meaning (CLAUDE.md UI rules):
    // green = verified/done/online, amber = needs attention.
    component StatusPill: Row {
        property string label: ""
        property bool ok: false
        spacing: Theme.s8
        Rectangle {
            width: Theme.s8; height: Theme.s8; radius: width / 2
            anchors.verticalCenter: parent.verticalCenter
            color: parent.ok ? Theme.accent : Theme.amber
        }
        Text {
            text: parent.label
            font.family: Theme.sans
            font.pixelSize: Theme.small
            color: parent.ok ? Theme.accent : Theme.amber
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: root.width
            spacing: Theme.s16

            ColumnLayout {
                Layout.margins: Theme.s24
                Layout.fillWidth: true
                spacing: Theme.s16

                Text {
                    text: "Open Labbook"
                    font.family: Theme.sans
                    font.pixelSize: Theme.pageTitle
                    font.weight: Theme.semibold
                    font.letterSpacing: Theme.titleSpacing
                    color: Theme.ink
                }
                Text {
                    text: "Phase 0 harness — not a product screen"
                    font.family: Theme.sans
                    font.pixelSize: Theme.meta
                    color: Theme.ink3
                }

                RowLayout {
                    spacing: Theme.s20
                    StatusPill { label: root.ready ? "olb_ui connected" : "connecting"; ok: root.ready }
                    StatusPill { label: root.coreReachable ? "olb_core reachable" : "olb_core unreachable"; ok: root.coreReachable }
                    StatusPill {
                        label: "delivery " + (root.statusObj.state || "unknown")
                        ok: root.statusObj.state === "online"
                    }
                    StatusPill {
                        label: "storage " + (root.statusObj.storage || "unknown")
                        ok: root.statusObj.storage === "online"
                    }
                }

                Text {
                    text: "Identity  " + (root.identity === "" ? "—" : root.identity)
                    font.family: Theme.mono
                    font.pixelSize: Theme.fingerprint
                    color: Theme.violet          // violet = committed/identity material
                    Layout.fillWidth: true
                    elide: Text.ElideMiddle
                }

                Button {
                    text: "Refresh"
                    implicitHeight: Theme.touchTarget
                    enabled: root.ready
                    onClicked: root.backend.refreshSessionStatus()
                }
            }

            // ---- Records (task 4) ----
            Panel {
                title: "Signed records"
                subtitle: root.recordModel.length + " received"
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s24
                Layout.rightMargin: Theme.s24

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s12
                    TextField {
                        id: recordTitle
                        placeholderText: "Record title"
                        implicitHeight: Theme.touchTarget
                        Layout.fillWidth: true
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                    }
                    Button {
                        text: "Publish signed record"
                        implicitHeight: Theme.touchTarget
                        enabled: root.ready && recordTitle.text.length > 0
                        onClicked: { root.backend.publishTestRecord(recordTitle.text); recordTitle.text = ""; }
                    }
                }

                Repeater {
                    model: root.recordModel
                    delegate: Text {
                        required property var modelData
                        Layout.fillWidth: true
                        font.family: Theme.sans
                        font.pixelSize: Theme.small
                        color: Theme.ink2
                        elide: Text.ElideRight
                        text: "• " + (modelData.payload ? modelData.payload.title : "?")
                              + "   " + String(modelData.author).substring(0, 16) + "…"
                    }
                }
            }

            // ---- Files (task 5) ----
            Panel {
                title: "Files in Logos Storage"
                subtitle: root.cidModel.length + " CIDs"
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s24
                Layout.rightMargin: Theme.s24
                Layout.bottomMargin: Theme.s24

                Text {
                    text: "Absolute paths only — the sandbox means the view never reads the file; "
                          + "olb_core does the I/O and the UI passes paths and CIDs."
                    font.family: Theme.sans
                    font.pixelSize: Theme.meta
                    color: Theme.ink3
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s12
                    TextField {
                        id: uploadPath
                        placeholderText: "Choose a file, or type an absolute path"
                        implicitHeight: Theme.touchTarget
                        Layout.fillWidth: true
                        font.family: Theme.mono
                        font.pixelSize: Theme.small
                    }
                    Button {
                        text: "Choose file…"
                        implicitHeight: Theme.touchTarget
                        enabled: root.ready
                        onClicked: uploadPicker.open()
                    }
                    Button {
                        text: "Upload"
                        implicitHeight: Theme.touchTarget
                        enabled: root.ready && uploadPath.text.length > 0
                        onClicked: root.backend.uploadFile(uploadPath.text)
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.s12
                    TextField {
                        id: fetchCid
                        placeholderText: "CID"
                        implicitHeight: Theme.touchTarget
                        Layout.preferredWidth: 220
                        font.family: Theme.mono
                        font.pixelSize: Theme.small
                    }
                    TextField {
                        id: fetchDest
                        placeholderText: "/absolute/destination"
                        implicitHeight: Theme.touchTarget
                        Layout.fillWidth: true
                        font.family: Theme.mono
                        font.pixelSize: Theme.small
                    }
                    Button {
                        text: "Choose folder…"
                        implicitHeight: Theme.touchTarget
                        enabled: root.ready
                        onClicked: destPicker.open()
                    }
                    Button {
                        text: "Fetch"
                        implicitHeight: Theme.touchTarget
                        enabled: root.ready && fetchCid.text.length > 0 && fetchDest.text.length > 0
                        onClicked: root.backend.fetchFile(fetchCid.text, fetchDest.text)
                    }
                }

                Repeater {
                    model: root.cidModel
                    delegate: Text {
                        required property var modelData
                        Layout.fillWidth: true
                        font.family: Theme.mono
                        font.pixelSize: Theme.meta
                        color: Theme.ink2
                        elide: Text.ElideMiddle
                        text: (modelData.direction === "upload" ? "↑ " : "↓ ") + modelData.cid
                    }
                }

                Text {
                    visible: root.lastStorageResult !== ""
                    readonly property var parsed: root.parseJson(root.lastStorageResult, null)
                    readonly property bool failed: parsed && parsed.ok === false
                    color: failed ? Theme.amber : Theme.ink3
                    text: failed && parsed.error ? parsed.error.message
                                                 : root.lastStorageResult
                    font.family: Theme.mono
                    font.pixelSize: Theme.meta
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }
        }
    }
}
