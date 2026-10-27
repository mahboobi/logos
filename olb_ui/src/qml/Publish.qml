import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S5 · Publish bundle (docs/03, design/canvas/Publish.dc.html).
//
// Choose what to reveal, check the credit table, publish the bundle publicly.
// Rendered from `publishData`, whose shape is fixtures/sample-project.json →
// publish.
//
// publish.execute is Phase 4, so "Publish bundle" emits publishRequested()
// rather than claiming anything was published. The copy-count rule from
// docs/03 S5 is enforced here, though: while an included file has fewer than
// two copies the button stays disabled and the row offers "Pin to replica".
Item {
    id: screen

    // ---- Interface contract ----
    property var publishData: ({})
    signal publishRequested(var options)
    signal pinRequested(string revealId, string cid)
    signal openBoard()

    // ---- Derived, all guarded: the bundle can be empty or arrive late ----
    readonly property var bundle: screen.publishData || ({})
    readonly property var chain: screen.bundle.chain || []
    readonly property var revealOptions: screen.bundle.reveal || []
    readonly property var credits: screen.bundle.credits || []
    readonly property var afterOptions: screen.bundle.after || []
    readonly property var manuscript: screen.bundle.manuscript || ({})
    readonly property var licences: screen.bundle.licences || []
    readonly property var notice: screen.bundle.notice || ({})
    readonly property string bundleName: screen.bundle.name || "Preprint bundle"
    readonly property bool twoColumn: screen.width > Theme.narrowBreakpoint

    // ---- Local choices ----
    // Reassigned as whole objects: mutating in place would not re-evaluate the
    // `checked` bindings or the publish gate.
    property var revealOn: ({})
    property var afterOn: ({})
    property string licence: ""

    function seed() {
        // Read through publishData, not the derived properties: this runs from
        // onPublishDataChanged, which can fire before they are bound.
        var data = screen.publishData || ({});
        var reveal = data.reveal || [];
        var r = {};
        for (var i = 0; i < reveal.length; i++)
            r[reveal[i].id] = reveal[i].checked === true;
        screen.revealOn = r;
        var after = data.after || [];
        var a = {};
        for (var j = 0; j < after.length; j++)
            a[after[j].id] = after[j].checked === true;
        screen.afterOn = a;
        var licences = data.licences || [];
        screen.licence = data.licence || (licences.length > 0 ? licences[0] : "");
    }
    function toggleReveal(id) {
        var copy = {};
        for (var k in screen.revealOn) copy[k] = screen.revealOn[k];
        copy[id] = !copy[id];
        screen.revealOn = copy;
    }
    function toggleAfter(id) {
        var copy = {};
        for (var k in screen.afterOn) copy[k] = screen.afterOn[k];
        copy[id] = !copy[id];
        screen.afterOn = copy;
    }
    function chosen(map) {
        var out = [];
        for (var k in map) if (map[k]) out.push(k);
        return out;
    }

    // docs/03 S5 rule: block publish while an included file has under 2 copies.
    readonly property var underCopied: {
        var out = [];
        var opts = screen.bundle.reveal || [];
        var on = screen.revealOn || ({});
        for (var i = 0; i < opts.length; i++) {
            var o = opts[i];
            if (on[o.id] && o.copies !== undefined && o.copies < 2)
                out.push(o);
        }
        return out;
    }
    readonly property bool blocked: (screen.underCopied || []).length > 0

    onPublishDataChanged: screen.seed()
    Component.onCompleted: screen.seed()

    // ---- One row of the credit table ----
    component CreditCell: Text {
        font.family: Theme.sans
        font.pixelSize: Theme.body
        color: Theme.ink
        wrapMode: Text.Wrap
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: screen.width
            spacing: Theme.s20

            // ---- Breadcrumb ----
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.topMargin: Theme.s28
                spacing: Theme.s6

                TextLink {
                    label: "Board"
                    size: Theme.small
                    weight: Theme.regular
                    compact: true
                    onClicked: screen.openBoard()
                }
                Text {
                    text: "/"
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink3
                    Accessible.ignored: true
                }
                Text {
                    text: "Publish"
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink3
                }
                Text {
                    text: "/"
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink3
                    Accessible.ignored: true
                }
                Text {
                    text: screen.bundleName
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    color: Theme.ink
                }
                Item { Layout.fillWidth: true }
            }

            // ---- Header ----
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.maximumWidth: 760            // design/canvas header max-width
                spacing: Theme.s8

                Text {
                    text: "Publish the preprint bundle"
                    font.family: Theme.sans
                    font.pixelSize: Theme.pageTitle
                    font.weight: Theme.semibold
                    font.letterSpacing: Theme.titleSpacing
                    color: Theme.ink
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                Text {
                    text: "Everything you include becomes public on Logos Storage and is "
                          + "recorded on LEZ, with its full history from idea to manuscript."
                    font.family: Theme.sans
                    font.pixelSize: Theme.lede
                    lineHeight: Theme.bodyLineHeight
                    color: Theme.ink2
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            // ---- Provenance chain ----
            Panel {
                title: "Provenance chain"
                subtitle: "Readers can trace the manuscript back to the first idea"
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32

                ProvenanceChain {
                    nodes: screen.chain
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.s2
                }
            }

            // ---- Reveal, credit and the publish panel ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s48
                columns: screen.twoColumn ? 2 : 1
                columnSpacing: Theme.s20
                rowSpacing: Theme.s20

                // ================= Main column =================
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s16

                    // ---- What to reveal ----
                    Panel {
                        title: "What to reveal"
                        Layout.fillWidth: true

                        Repeater {
                            model: screen.revealOptions
                            delegate: CheckRow {
                                required property var modelData
                                required property int index

                                Layout.fillWidth: true
                                title: modelData.title || ""
                                note: modelData.note || ""
                                noteKind: modelData.noteKind || "neutral"
                                actionText: modelData.action || ""
                                checked: screen.revealOn[modelData.id] === true
                                divider: index < screen.revealOptions.length - 1
                                onToggled: screen.toggleReveal(modelData.id)
                                onActionClicked: screen.pinRequested(modelData.id,
                                                                     modelData.cid || "")
                            }
                        }
                    }

                    // ---- Contributors and credit ----
                    Panel {
                        id: creditPanel
                        title: "Contributors and credit"
                        subtitle: "Each person accepts or contests their record"
                        Layout.fillWidth: true

                        // Contributor / Role / Recorded work / Status.
                        readonly property var shares: [0.26, 0.30, 0.20, 0.24]
                        readonly property bool stacked: screen.width < Theme.narrowBreakpoint
                        function colWidth(i) {
                            return Math.max(72, (creditPanel.width - Theme.s20 * 2
                                                 - Theme.s12 * 3) * creditPanel.shares[i]);
                        }

                        // ---- Header row (wide only) ----
                        RowLayout {
                            visible: !creditPanel.stacked
                            Layout.fillWidth: true
                            spacing: Theme.s12

                            Repeater {
                                model: ["Contributor", "Role (CRediT)", "Recorded work", "Status"]
                                delegate: Text {
                                    required property var modelData
                                    required property int index
                                    text: modelData
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.meta
                                    font.weight: Theme.medium
                                    color: Theme.ink3
                                    Layout.preferredWidth: creditPanel.colWidth(index)
                                    wrapMode: Text.Wrap
                                }
                            }
                        }
                        Rectangle {
                            visible: !creditPanel.stacked
                            Layout.fillWidth: true
                            Layout.preferredHeight: 1
                            color: Theme.line
                        }

                        // ---- Rows ----
                        Repeater {
                            model: screen.credits

                            delegate: ColumnLayout {
                                id: creditRow
                                required property var modelData
                                required property int index

                                Layout.fillWidth: true
                                spacing: Theme.s4

                                Accessible.role: Accessible.StaticText
                                Accessible.name: (modelData.name || "") + ", "
                                    + (modelData.roles || "") + ", "
                                    + (modelData.work || "") + ", "
                                    + (modelData.status || "")

                                // Wide: four columns. Narrow: the same four
                                // facts stacked, so nothing scrolls sideways.
                                RowLayout {
                                    visible: !creditPanel.stacked
                                    Layout.fillWidth: true
                                    spacing: Theme.s12

                                    ColumnLayout {
                                        Layout.preferredWidth: creditPanel.colWidth(0)
                                        spacing: 0
                                        CreditCell {
                                            text: modelData.name || ""
                                            font.weight: Theme.medium
                                            Layout.fillWidth: true
                                        }
                                        Text {
                                            visible: (modelData.note || "") !== ""
                                            text: modelData.note || ""
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.meta
                                            color: Theme.ink3
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                    }
                                    CreditCell {
                                        text: modelData.roles || ""
                                        Layout.preferredWidth: creditPanel.colWidth(1)
                                    }
                                    CreditCell {
                                        text: modelData.work || ""
                                        Layout.preferredWidth: creditPanel.colWidth(2)
                                    }
                                    Item {
                                        Layout.preferredWidth: creditPanel.colWidth(3)
                                        Layout.preferredHeight: statusBadge.implicitHeight
                                        Badge {
                                            id: statusBadge
                                            anchors.left: parent.left
                                            text: modelData.status || ""
                                            kind: modelData.kind || "neutral"
                                        }
                                    }
                                }

                                ColumnLayout {
                                    visible: creditPanel.stacked
                                    Layout.fillWidth: true
                                    spacing: Theme.s2

                                    CreditCell {
                                        text: modelData.name || ""
                                        font.weight: Theme.medium
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        visible: (modelData.note || "") !== ""
                                        text: modelData.note || ""
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        text: (modelData.roles || "") + " · "
                                              + (modelData.work || "")
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.small
                                        color: Theme.ink2
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                    Badge {
                                        text: modelData.status || ""
                                        kind: modelData.kind || "neutral"
                                        Layout.topMargin: Theme.s2
                                    }
                                }

                                Rectangle {
                                    visible: creditRow.index < screen.credits.length - 1
                                    Layout.fillWidth: true
                                    Layout.topMargin: Theme.s4
                                    Layout.preferredHeight: 1
                                    color: Theme.lineSoft
                                }
                            }
                        }
                    }
                }

                // ================= Publish panel =================
                Panel {
                    title: "Publish"
                    Layout.fillWidth: !screen.twoColumn
                    Layout.preferredWidth: screen.twoColumn ? 340 : -1
                    Layout.alignment: Qt.AlignTop

                    // ---- Manuscript file ----
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: fileCol.implicitHeight + Theme.s12 * 2
                        radius: Theme.rControl
                        color: Theme.ground

                        ColumnLayout {
                            id: fileCol
                            anchors.fill: parent
                            anchors.margins: Theme.s12
                            spacing: Theme.s2

                            Text {
                                text: screen.manuscript.name || ""
                                font.family: Theme.sans
                                font.pixelSize: Theme.body
                                font.weight: Theme.medium
                                color: Theme.ink
                                wrapMode: Text.WrapAnywhere
                                Layout.fillWidth: true
                            }
                            Text {
                                text: (screen.manuscript.role || "Manuscript")
                                      + " · " + (screen.manuscript.size || "")
                                font.family: Theme.sans
                                font.pixelSize: Theme.meta
                                color: Theme.ink3
                                Layout.fillWidth: true
                            }
                        }
                    }

                    // ---- Licence ----
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.s6

                        FieldLabel { text: "Licence" }
                        ComboBox {
                            id: licenceBox
                            Layout.fillWidth: true
                            implicitHeight: Theme.touchTarget
                            model: screen.licences
                            currentIndex: Math.max(0, screen.licences.indexOf(screen.licence))
                            Accessible.name: "Licence"
                            onActivated: screen.licence = screen.licences[currentIndex]
                        }
                    }

                    // ---- After publishing ----
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: Theme.s4
                        spacing: Theme.s4

                        FieldLabel { text: "After publishing" }

                        Repeater {
                            model: screen.afterOptions
                            delegate: CheckRow {
                                required property var modelData
                                Layout.fillWidth: true
                                title: modelData.label || ""
                                checked: screen.afterOn[modelData.id] === true
                                pad: Theme.s4
                                onToggled: screen.toggleAfter(modelData.id)
                            }
                        }
                    }

                    // ---- Credit still open ----
                    Rectangle {
                        visible: (screen.notice.text || "") !== ""
                        Layout.fillWidth: true
                        implicitHeight: noticeText.implicitHeight + Theme.s12 * 2
                        radius: Theme.rControl
                        color: Theme.amberTint

                        Text {
                            id: noticeText
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.margins: Theme.s12
                            text: screen.notice.text || ""
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            lineHeight: Theme.bodyLineHeight
                            color: Theme.amber
                            wrapMode: Text.Wrap
                        }
                    }

                    // ---- Copies gate (docs/03 S5) ----
                    Rectangle {
                        visible: screen.blocked
                        Layout.fillWidth: true
                        implicitHeight: copiesText.implicitHeight + Theme.s12 * 2
                        radius: Theme.rControl
                        color: Theme.amberTint

                        Text {
                            id: copiesText
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.margins: Theme.s12
                            text: screen.blocked
                                  ? (screen.underCopied[0].note || "")
                                  : ""
                            font.family: Theme.sans
                            font.pixelSize: Theme.small
                            lineHeight: Theme.bodyLineHeight
                            color: Theme.amber
                            wrapMode: Text.Wrap
                        }
                    }

                    PrimaryButton {
                        text: "Publish bundle"
                        Layout.fillWidth: true
                        enabled: !screen.blocked
                        accessibleName: screen.blocked
                            ? "Publish bundle, unavailable until every included file has "
                              + "a second copy"
                            : "Publish bundle"
                        onClicked: screen.publishRequested({
                            reveal: screen.chosen(screen.revealOn),
                            licence: screen.licence,
                            after: screen.chosen(screen.afterOn)
                        })
                    }

                    Text {
                        text: "Published files cannot be recalled. Copies spread as other "
                              + "nodes download them."
                        font.family: Theme.sans
                        font.pixelSize: Theme.meta
                        lineHeight: Theme.bodyLineHeight
                        color: Theme.ink3
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
