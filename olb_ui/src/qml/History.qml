import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// S6 · History (version control).
// Spec: docs/03-screens.md "S6 · History"; visuals and copy:
// design/canvas/History.dc.html; model: docs/05-version-control.md.
//
// Colour carries meaning here (CLAUDE.md UI rules):
//   violet = commit, hash, branch (committed, not yet anchored)
//   accent = anchored / verified / added
//   amber  = needs attention (waiting for an anchor, removed text, conflicts)
//   slate  = neutral (merge, negative result, protected)
Item {
    id: page

    // ---- Interface contract. Nothing else drives this screen. ----
    // `backend` is the olb_ui C++ backend (QML --QtRO--> olb_ui --> olb_core).
    // Every vcs.* method docs/03 S6 lists is still unimplemented, so the screen
    // renders from `history` only and never calls a method that does not exist.
    property var backend: null
    property var history: ({})
    property string filterIdeaId: ""
    signal openCommit(string commitId)
    signal openRecord(string recordId)

    // ---- Data, all guarded: `history` may be empty or partial. ----
    readonly property var commits: page.history.commits || []
    readonly property var diff: page.history.diff || ({})
    readonly property var mergeRequest: page.history.mergeRequest || null
    readonly property var mr: page.mergeRequest || ({})
    readonly property var branches: page.history.branches || []
    readonly property var tags: page.history.tags || []

    readonly property var diffFiles: page.diff.files || ({})
    readonly property var filesAdded: page.diffFiles.added || []
    readonly property var filesRemoved: page.diffFiles.removed || []
    readonly property var filesChanged: page.diffFiles.changed || []

    // ---- Selection ----
    // The core marks one commit `selected`; a click overrides it locally and
    // asks the host for that commit's diff through openCommit().
    property string selectedCommit: ""
    readonly property string markedCommit: {
        for (var i = 0; i < page.commits.length; i++)
            if (page.commits[i].selected) return page.commits[i].id;
        return page.commits.length > 0 ? page.commits[0].id : "";
    }
    readonly property string activeCommit: page.selectedCommit !== ""
                                           ? page.selectedCommit : page.markedCommit

    function pickCommit(commitId) {
        page.selectedCommit = commitId;
        page.openCommit(commitId);
    }

    // ---- Branch filter ----
    property int branchIndex: 0
    readonly property var branchLabels: {
        var out = [];
        for (var i = 0; i < page.branches.length; i++) {
            var b = page.branches[i];
            out.push(b.status === "Merged" ? b.name + " (merged)" : b.name);
        }
        return out.length > 0 ? out : ["main"];
    }
    readonly property string currentBranch: {
        if (page.branchIndex >= 0 && page.branchIndex < page.branches.length)
            return page.branches[page.branchIndex].name;
        return "main";
    }

    // Idea scope. The fixture carries no idea title for a commit, so a scoped
    // view can only name the id it was given.
    // TODO(phase1b): vcs.log({ ideaId, branch, limit, before }) to re-query the
    // log, plus a record title for `filterIdeaId`.
    readonly property var ideaOptions: page.filterIdeaId !== ""
                                       ? [page.filterIdeaId, "All ideas in this project"]
                                       : ["All ideas in this project"]

    readonly property bool wide: page.width > Theme.narrowBreakpoint

    // ---- Commit graph -------------------------------------------------------
    // Lanes come from the core (docs/03 S6 "compute lanes in core so QML only
    // draws"). Three lanes are drawn; anything further collapses into a
    // "+n branches" marker under the list.
    readonly property int maxLanes: 3
    readonly property int laneStep: Theme.s24
    readonly property int graphWidth: Theme.s48

    function laneX(l) { return Theme.s12 + page.laneStep * Math.min(l, page.maxLanes - 1); }
    function laneOf(c) { return Math.min(c.lane || 0, page.maxLanes - 1); }
    function laneColor(l) { return l === 0 ? Theme.accent : Theme.violet; }

    // Trunk node = anchored (accent). Branch lane, or a commit still waiting
    // for its batch, = committed but not anchored (violet).
    function nodeColor(c) {
        if (page.laneOf(c) > 0) return Theme.violet;
        var b = c.badge || ({});
        return b.kind === "attention" ? Theme.violet : Theme.accent;
    }

    readonly property int laneCount: {
        var n = 1;
        for (var i = 0; i < page.commits.length; i++) {
            var c = page.commits[i];
            n = Math.max(n, (c.lane || 0) + 1);
            if (c.merge) n = Math.max(n, (c.merge.fromLane || 0) + 1);
            if (c.fork) n = Math.max(n, (c.fork.toLane || 0) + 1);
        }
        return n;
    }

    // One descriptor per row: which lanes have a line above/below the node, and
    // the horizontal connector of a fork or merge.
    readonly property var graphRows: page.buildGraph(page.commits)

    function buildGraph(cs) {
        var rows = [];
        var i, j, l;
        for (i = 0; i < cs.length; i++)
            rows.push({ up: [false, false, false],
                        down: [false, false, false],
                        hbarA: -1, hbarB: -1 });

        function laneAt(k) { return Math.min(cs[k].lane || 0, 2); }
        // `a` is the newer row, `b` the older one: the line runs down from a's
        // node, through every row between, up into b's node.
        function link(a, b, lane) {
            rows[a].down[lane] = true;
            rows[b].up[lane] = true;
            for (var t = a + 1; t < b; t++) {
                rows[t].up[lane] = true;
                rows[t].down[lane] = true;
            }
        }

        // 1. first parent: the next older commit on the same lane.
        var last = ({});
        for (i = 0; i < cs.length; i++) {
            l = laneAt(i);
            if (last[l] !== undefined) link(last[l], i, l);
            last[l] = i;
        }
        // 2. merge commits have a second parent on another lane; fork rows join
        //    back to the lane they branched from.
        for (i = 0; i < cs.length; i++) {
            l = laneAt(i);
            if (cs[i].merge) {
                var fl = Math.min(cs[i].merge.fromLane || 0, 2);
                for (j = i + 1; j < cs.length; j++) {
                    if (laneAt(j) === fl) { link(i, j, fl); break; }
                }
                rows[i].hbarA = Math.min(l, fl);
                rows[i].hbarB = Math.max(l, fl);
            }
            if (cs[i].fork) {
                var tl = Math.min(cs[i].fork.toLane || 0, 2);
                rows[i].hbarA = Math.min(l, tl);
                rows[i].hbarB = Math.max(l, tl);
            }
        }
        return rows;
    }

    // ---- Copy helpers -------------------------------------------------------
    function plural(n, one, many) { return n === 1 ? one : many; }

    function reviewerState(state) {
        if (state === "approved") return "Approved";
        if (state === "requested") return "Review requested";
        return state;
    }

    readonly property string diffRecord: {
        var ch = page.diff.changes || [];
        return ch.length > 0 ? (ch[0].record || "") : "";
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: page.width
            spacing: Theme.s20

            // ---- 1. Header ----
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.topMargin: Theme.s28
                spacing: Theme.s16

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignBottom
                    spacing: Theme.s8

                    Text {
                        text: "History"
                        font.family: Theme.sans
                        font.pixelSize: Theme.pageTitle
                        font.weight: Theme.semibold
                        font.letterSpacing: Theme.titleSpacing
                        color: Theme.ink
                    }
                    Text {
                        text: "Every change is a signed commit. Branch to try something "
                              + "without touching the main record, then merge it after review. "
                              + "Commits are anchored on LEZ in batches."
                        font.family: Theme.sans
                        font.pixelSize: Theme.lede
                        color: Theme.ink2
                        lineHeight: Theme.bodyLineHeight
                        lineHeightMode: Text.ProportionalHeight
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                        Layout.maximumWidth: 720   // design/canvas: lede max-width
                    }
                }

                Button {
                    text: "Compare versions"
                    implicitHeight: Theme.touchTarget
                    Layout.alignment: Qt.AlignBottom
                    Accessible.name: "Compare versions"
                    onClicked: {
                        // TODO(phase1b): a compare view over vcs.diff(commitId)
                        // for two chosen commits. No backend method yet.
                    }
                }
                // Accent primary. A Controls Button cannot be restyled under the
                // platform style this app runs with (its `highlighted` fill is
                // the system accent, which is not in the palette), so primary
                // actions are built from a focusable Rectangle — the pattern
                // FilterPills and ChoiceCard already use.
                Rectangle {
                    id: newBranchButton
                    Layout.alignment: Qt.AlignBottom
                    implicitHeight: Theme.touchTarget
                    implicitWidth: newBranchLabel.implicitWidth + Theme.s20 * 2
                    radius: Theme.rControl
                    color: Theme.accent
                    opacity: newBranchPress.pressed ? 0.88 : 1

                    function activate() {
                        // TODO(phase1b): vcs.branch(name, fromCommit).
                    }

                    Accessible.role: Accessible.Button
                    Accessible.name: "New branch"
                    Accessible.onPressAction: newBranchButton.activate()
                    activeFocusOnTab: true

                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -3
                        radius: Theme.rControl
                        color: "transparent"
                        border.width: 2
                        border.color: Theme.violet
                        visible: parent.activeFocus
                    }

                    Text {
                        id: newBranchLabel
                        anchors.centerIn: parent
                        text: "New branch"
                        font.family: Theme.sans
                        font.pixelSize: Theme.body
                        font.weight: Theme.semibold
                        color: Theme.textOnAccent
                    }

                    MouseArea {
                        id: newBranchPress
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: newBranchButton.activate()
                    }
                    Keys.onReturnPressed: newBranchButton.activate()
                    Keys.onSpacePressed: newBranchButton.activate()
                }
            }

            // ---- 2. Filters ----
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                spacing: Theme.s10

                FieldLabel { text: "Branch" }
                ComboBox {
                    id: branchSelect
                    implicitHeight: Theme.touchTarget
                    font.family: Theme.mono       // branch names are mono
                    font.pixelSize: Theme.small
                    model: page.branchLabels
                    currentIndex: page.branchIndex
                    Accessible.name: "Branch"
                    onActivated: function (i) {
                        page.branchIndex = i;
                        // TODO(phase1b): vcs.log({ branch }) — the list below is
                        // whatever the core last pushed, not a re-query.
                    }
                }

                FieldLabel {
                    text: "Idea"
                    Layout.leftMargin: Theme.s8
                }
                ComboBox {
                    id: ideaSelect
                    implicitHeight: Theme.touchTarget
                    font.family: Theme.sans
                    font.pixelSize: Theme.small
                    Layout.maximumWidth: 320
                    model: page.ideaOptions
                    Accessible.name: "Idea"
                    onActivated: function (i) {
                        // TODO(phase1b): vcs.log({ ideaId }) to scope the log.
                    }
                }
                Item { Layout.fillWidth: true }
            }

            // ---- 3. Main column + side column ----
            GridLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.s32
                Layout.rightMargin: Theme.s32
                Layout.bottomMargin: Theme.s48
                columns: page.wide ? 2 : 1
                columnSpacing: Theme.s20
                rowSpacing: Theme.s20

                // ======================= MAIN COLUMN =======================
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    spacing: Theme.s16

                    // ---- Panel: commits on {branch} ----
                    Panel {
                        title: "Commits on " + page.currentBranch
                        Layout.fillWidth: true

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0

                            Repeater {
                                model: page.commits

                                delegate: RowLayout {
                                    id: commitRow
                                    required property var modelData
                                    required property int index

                                    readonly property var graph: page.graphRows[index]
                                        || ({ up: [], down: [], hbarA: -1, hbarB: -1 })
                                    readonly property int lane: page.laneOf(modelData)
                                    readonly property var badgeData: modelData.badge || ({})
                                    readonly property string badgeKind: badgeData.kind || ""
                                    readonly property bool monoBadge: badgeKind === "branch"
                                                                      || badgeKind === "tag"
                                    readonly property bool isSelected: page.activeCommit === modelData.id
                                    readonly property bool isLast: index === page.commits.length - 1

                                    Layout.fillWidth: true
                                    spacing: Theme.s12

                                    // ---- Graph gutter (decorative; the row below carries the name) ----
                                    Item {
                                        id: gutter
                                        Layout.preferredWidth: page.graphWidth
                                        Layout.minimumWidth: page.graphWidth
                                        Layout.fillHeight: true

                                        Repeater {
                                            model: page.maxLanes
                                            delegate: Item {
                                                required property int index

                                                Rectangle {   // line up to the newer commit
                                                    visible: commitRow.graph.up[index] === true
                                                    x: page.laneX(index) - 1
                                                    y: 0
                                                    width: 2
                                                    height: gutter.height / 2
                                                    color: page.laneColor(index)
                                                }
                                                Rectangle {   // line down to the older commit
                                                    visible: commitRow.graph.down[index] === true
                                                    x: page.laneX(index) - 1
                                                    y: gutter.height / 2
                                                    width: 2
                                                    height: gutter.height / 2
                                                    color: page.laneColor(index)
                                                }
                                            }
                                        }

                                        Rectangle {   // fork or merge connector
                                            visible: commitRow.graph.hbarA >= 0
                                                     && commitRow.graph.hbarB >= 0
                                            x: page.laneX(Math.max(0, commitRow.graph.hbarA))
                                            y: gutter.height / 2 - 1
                                            width: Math.max(0, page.laneX(Math.max(0, commitRow.graph.hbarB))
                                                               - page.laneX(Math.max(0, commitRow.graph.hbarA)))
                                            height: 2
                                            color: Theme.violet
                                        }

                                        Rectangle {   // the commit node
                                            readonly property bool isMerge: !!commitRow.modelData.merge
                                            width: isMerge ? Theme.s16 : Theme.s12
                                            height: width
                                            radius: width / 2
                                            x: page.laneX(commitRow.lane) - width / 2
                                            y: gutter.height / 2 - width / 2
                                            color: isMerge ? page.nodeColor(commitRow.modelData)
                                                           : Theme.surface
                                            border.width: isMerge ? 0 : 2
                                            border.color: page.nodeColor(commitRow.modelData)
                                        }
                                    }

                                    // ---- Commit row ----
                                    Rectangle {
                                        id: rowCard
                                        Layout.fillWidth: true
                                        implicitHeight: rowBody.implicitHeight + Theme.s12 * 2
                                        color: commitRow.isSelected ? Theme.violetTint : "transparent"
                                        radius: commitRow.isSelected ? Theme.rControl : 0

                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Commit " + commitRow.modelData.id
                                                         + ", " + (commitRow.modelData.message || "")
                                        Accessible.description: commitRow.modelData.meta || ""
                                        Accessible.onPressAction: page.pickCommit(commitRow.modelData.id)
                                        activeFocusOnTab: true

                                        Rectangle {   // row separator
                                            visible: !commitRow.isSelected && !commitRow.isLast
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            height: 1
                                            color: Theme.lineSoft
                                        }

                                        Rectangle {   // focus ring, keyboard users need it
                                            anchors.fill: parent
                                            anchors.margins: -2
                                            radius: Theme.rControl
                                            color: "transparent"
                                            border.width: 2
                                            border.color: Theme.violet
                                            visible: parent.activeFocus
                                        }

                                        ColumnLayout {
                                            id: rowBody
                                            anchors.fill: parent
                                            anchors.leftMargin: Theme.s10
                                            anchors.rightMargin: Theme.s10
                                            anchors.topMargin: Theme.s12
                                            anchors.bottomMargin: Theme.s12
                                            spacing: Theme.s8

                                            RowLayout {
                                                Layout.fillWidth: true
                                                spacing: Theme.s12

                                                Text {   // short hash
                                                    text: commitRow.modelData.id || ""
                                                    font.family: Theme.mono
                                                    font.pixelSize: Theme.meta
                                                    font.weight: Theme.medium
                                                    color: Theme.violet
                                                }
                                                Text {
                                                    text: commitRow.modelData.message || ""
                                                    font.family: Theme.sans
                                                    font.pixelSize: Theme.body
                                                    font.weight: Theme.medium
                                                    color: Theme.ink
                                                    wrapMode: Text.Wrap
                                                    Layout.fillWidth: true
                                                }
                                                Badge {
                                                    visible: commitRow.badgeKind !== ""
                                                             && !commitRow.monoBadge
                                                    text: commitRow.badgeData.text || ""
                                                    kind: commitRow.badgeKind
                                                }
                                                Rectangle {   // mono badge: branch name or tag
                                                    visible: commitRow.monoBadge
                                                    implicitHeight: Theme.chipHeight
                                                    implicitWidth: monoBadgeLabel.implicitWidth + Theme.s12 * 2
                                                    radius: Theme.rPill
                                                    color: commitRow.badgeKind === "tag"
                                                           ? Theme.accentTint : Theme.violetTint
                                                    Text {
                                                        id: monoBadgeLabel
                                                        anchors.centerIn: parent
                                                        text: commitRow.badgeData.text || ""
                                                        font.family: Theme.mono
                                                        font.pixelSize: Theme.meta
                                                        color: commitRow.badgeKind === "tag"
                                                               ? Theme.accent : Theme.violet
                                                    }
                                                }
                                            }

                                            Text {
                                                text: commitRow.modelData.meta || ""
                                                font.family: Theme.sans
                                                font.pixelSize: Theme.meta
                                                color: Theme.ink2
                                                wrapMode: Text.Wrap
                                                Layout.fillWidth: true
                                            }
                                        }

                                        MouseArea {
                                            anchors.fill: parent
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: page.pickCommit(commitRow.modelData.id)
                                        }
                                        Keys.onReturnPressed: page.pickCommit(commitRow.modelData.id)
                                        Keys.onSpacePressed: page.pickCommit(commitRow.modelData.id)
                                    }
                                }
                            }
                        }

                        // Lanes past the third are collapsed (docs/03 S6).
                        Text {
                            visible: page.laneCount > page.maxLanes
                            text: "+" + (page.laneCount - page.maxLanes) + " branches"
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            font.weight: Theme.medium
                            color: Theme.slate
                            Layout.leftMargin: Theme.s12
                        }

                        Text {
                            visible: page.commits.length === 0
                            text: "No commits on this branch yet."
                            font.family: Theme.sans
                            font.pixelSize: Theme.body
                            color: Theme.ink3
                            Layout.fillWidth: true
                        }

                        // A text link, 44 px tall. Controls cannot be restyled on
                        // the platform style this app runs under, so links and
                        // violet buttons are built from a focusable Rectangle —
                        // the same pattern as FilterPills and ChoiceCard.
                        Rectangle {
                            id: olderLink
                            visible: page.commits.length > 0
                            Layout.alignment: Qt.AlignHCenter
                            implicitHeight: Theme.touchTarget
                            implicitWidth: olderLabel.implicitWidth + Theme.s16 * 2
                            radius: Theme.rControl
                            color: "transparent"

                            function activate() {
                                // TODO(phase1b): vcs.log({ branch, limit, before })
                                // for paging. The count is the design's copy until
                                // the core reports how many older commits exist.
                            }

                            Accessible.role: Accessible.Link
                            Accessible.name: "Show 40 older commits"
                            Accessible.onPressAction: olderLink.activate()
                            activeFocusOnTab: true

                            Rectangle {
                                anchors.fill: parent
                                radius: Theme.rControl
                                color: "transparent"
                                border.width: 2
                                border.color: Theme.violet
                                visible: parent.activeFocus
                            }

                            Text {
                                id: olderLabel
                                anchors.centerIn: parent
                                text: "Show 40 older commits"
                                font.family: Theme.sans
                                font.pixelSize: Theme.body
                                font.weight: Theme.medium
                                color: Theme.accent
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: olderLink.activate()
                            }
                            Keys.onReturnPressed: olderLink.activate()
                            Keys.onSpacePressed: olderLink.activate()
                        }
                    }

                    // ---- Panel: changes in {hash} ----
                    // Panel's title is plain text and this heading carries a mono
                    // violet hash, so the card chrome is spelled out here.
                    Rectangle {
                        Layout.fillWidth: true
                        visible: !!page.diff.commit
                        color: Theme.surface
                        radius: Theme.rPanel
                        border.width: 1
                        border.color: Theme.line
                        implicitHeight: diffBody.implicitHeight + Theme.s20 * 2

                        ColumnLayout {
                            id: diffBody
                            anchors.fill: parent
                            anchors.margins: Theme.s20
                            spacing: Theme.s14

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s8

                                Text {
                                    text: "Changes in"
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.sectionTitle
                                    font.weight: Theme.semibold
                                    color: Theme.ink
                                }
                                Text {
                                    text: page.diff.commit || ""
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.sectionTitle
                                    font.weight: Theme.semibold
                                    color: Theme.violet
                                }
                                Item { Layout.fillWidth: true }

                                Text {
                                    text: "Parent"
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                }
                                Text {
                                    text: page.diff.parent || ""
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                }
                                Text {
                                    text: "· signed with key"
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                }
                                Text {
                                    text: page.diff.signer || ""
                                    font.family: Theme.mono
                                    font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                }
                            }

                            // ---- DiffBlock per changed field ----
                            Repeater {
                                model: page.diff.changes || []

                                delegate: Rectangle {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    color: Theme.surface
                                    radius: Theme.rControl
                                    border.width: 1
                                    border.color: Theme.line
                                    clip: true
                                    implicitHeight: diffBlock.implicitHeight

                                    ColumnLayout {
                                        id: diffBlock
                                        anchors.left: parent.left
                                        anchors.right: parent.right
                                        anchors.top: parent.top
                                        spacing: 0

                                        Rectangle {   // record · field
                                            Layout.fillWidth: true
                                            color: Theme.ground
                                            implicitHeight: diffHead.implicitHeight + Theme.s8 * 2
                                            Text {
                                                id: diffHead
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.verticalCenter: parent.verticalCenter
                                                anchors.leftMargin: Theme.s12
                                                anchors.rightMargin: Theme.s12
                                                text: (modelData.record || "") + " · " + (modelData.field || "")
                                                font.family: Theme.sans
                                                font.pixelSize: Theme.small
                                                font.weight: Theme.medium
                                                color: Theme.ink
                                                wrapMode: Text.Wrap
                                            }
                                            Rectangle {
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.bottom: parent.bottom
                                                height: 1
                                                color: Theme.line
                                            }
                                        }

                                        Rectangle {   // removed
                                            Layout.fillWidth: true
                                            visible: (modelData.before || "") !== ""
                                            color: Theme.amberTint
                                            implicitHeight: beforeText.implicitHeight + Theme.s4 * 2
                                            Text {
                                                id: beforeText
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.top: parent.top
                                                anchors.leftMargin: Theme.s12
                                                anchors.rightMargin: Theme.s12
                                                anchors.topMargin: Theme.s4
                                                text: "− " + (modelData.before || "")
                                                font.family: Theme.mono
                                                font.pixelSize: Theme.meta
                                                color: Theme.amber
                                                lineHeight: Theme.bodyLineHeight
                                                lineHeightMode: Text.ProportionalHeight
                                                wrapMode: Text.Wrap
                                            }
                                        }

                                        Rectangle {   // added
                                            Layout.fillWidth: true
                                            visible: (modelData.after || "") !== ""
                                            color: Theme.accentTint
                                            implicitHeight: afterText.implicitHeight + Theme.s4 * 2
                                            Text {
                                                id: afterText
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.top: parent.top
                                                anchors.leftMargin: Theme.s12
                                                anchors.rightMargin: Theme.s12
                                                anchors.topMargin: Theme.s4
                                                text: "+ " + (modelData.after || "")
                                                font.family: Theme.mono
                                                font.pixelSize: Theme.meta
                                                color: Theme.accent
                                                lineHeight: Theme.bodyLineHeight
                                                lineHeightMode: Text.ProportionalHeight
                                                wrapMode: Text.Wrap
                                            }
                                        }
                                    }
                                }
                            }

                            // ---- Files block ----
                            Rectangle {
                                Layout.fillWidth: true
                                visible: page.filesAdded.length + page.filesRemoved.length
                                         + page.filesChanged.length > 0
                                color: Theme.surface
                                radius: Theme.rControl
                                border.width: 1
                                border.color: Theme.line
                                clip: true
                                implicitHeight: filesBlock.implicitHeight

                                ColumnLayout {
                                    id: filesBlock
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    spacing: 0

                                    Rectangle {
                                        Layout.fillWidth: true
                                        color: Theme.ground
                                        implicitHeight: filesHead.implicitHeight + Theme.s8 * 2
                                        Text {
                                            id: filesHead
                                            anchors.left: parent.left
                                            anchors.leftMargin: Theme.s12
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: "Files"
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.small
                                            font.weight: Theme.medium
                                            color: Theme.ink
                                        }
                                        Rectangle {
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            height: 1
                                            color: Theme.line
                                        }
                                    }

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        Layout.topMargin: Theme.s4
                                        Layout.bottomMargin: Theme.s4
                                        spacing: Theme.s2

                                        Repeater {
                                            model: page.filesAdded
                                            delegate: Text {
                                                required property var modelData
                                                Layout.fillWidth: true
                                                Layout.leftMargin: Theme.s12
                                                Layout.rightMargin: Theme.s12
                                                text: "+ " + modelData
                                                font.family: Theme.mono
                                                font.pixelSize: Theme.meta
                                                color: Theme.accent
                                                wrapMode: Text.Wrap
                                            }
                                        }
                                        Repeater {
                                            model: page.filesRemoved
                                            delegate: Text {
                                                required property var modelData
                                                Layout.fillWidth: true
                                                Layout.leftMargin: Theme.s12
                                                Layout.rightMargin: Theme.s12
                                                text: "− " + modelData
                                                font.family: Theme.mono
                                                font.pixelSize: Theme.meta
                                                color: Theme.amber
                                                wrapMode: Text.Wrap
                                            }
                                        }
                                        Repeater {
                                            model: page.filesChanged
                                            delegate: Text {
                                                required property var modelData
                                                Layout.fillWidth: true
                                                Layout.leftMargin: Theme.s12
                                                Layout.rightMargin: Theme.s12
                                                text: "~ " + modelData
                                                font.family: Theme.mono
                                                font.pixelSize: Theme.meta
                                                color: Theme.slate
                                                wrapMode: Text.Wrap
                                            }
                                        }
                                    }
                                }
                            }

                            // ---- Diff actions ----
                            Flow {
                                Layout.fillWidth: true
                                spacing: Theme.s8

                                Button {
                                    visible: page.diffRecord !== ""
                                    text: "Open " + page.diffRecord
                                    implicitHeight: Theme.touchTarget
                                    Accessible.name: "Open " + page.diffRecord
                                    // TODO(phase1b): vcs.diff(commitId) should return the
                                    // record id; the fixture only carries its label.
                                    onClicked: page.openRecord(page.diffRecord)
                                }
                                Button {
                                    text: "Restore this version"
                                    implicitHeight: Theme.touchTarget
                                    Accessible.name: "Restore this version"
                                    onClicked: {
                                        // TODO(phase1b): vcs.restore(commitId).
                                    }
                                }
                                Button {
                                    text: "Branch from here"
                                    implicitHeight: Theme.touchTarget
                                    Accessible.name: "Branch from here"
                                    onClicked: {
                                        // TODO(phase1b): vcs.branch(name, fromCommit).
                                    }
                                }
                            }

                            Text {
                                text: "Restoring creates a new commit. Nothing in the history is ever rewritten."
                                font.family: Theme.sans
                                font.pixelSize: Theme.meta
                                color: Theme.ink3
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }
                        }
                    }
                }

                // ======================= SIDE COLUMN =======================
                ColumnLayout {
                    Layout.alignment: Qt.AlignTop
                    Layout.fillWidth: !page.wide
                    Layout.preferredWidth: 340          // design/canvas: side column basis
                    Layout.maximumWidth: page.wide ? 340 : Number.POSITIVE_INFINITY
                    spacing: Theme.s16

                    // ---- Open merge request (violet 2 px card) ----
                    Rectangle {
                        Layout.fillWidth: true
                        visible: !!page.mergeRequest
                        color: Theme.surface
                        radius: Theme.rPanel
                        border.width: 2
                        border.color: Theme.violet
                        implicitHeight: mrBody.implicitHeight + Theme.s20 * 2

                        ColumnLayout {
                            id: mrBody
                            anchors.fill: parent
                            anchors.margins: Theme.s20
                            spacing: Theme.s12

                            Text {
                                text: "Open merge request"
                                font.family: Theme.sans
                                font.pixelSize: Theme.meta
                                font.weight: Theme.semibold
                                color: Theme.violet
                            }
                            Text {
                                text: page.mr.title || ""
                                font.family: Theme.sans
                                font.pixelSize: Theme.sectionTitle
                                font.weight: Theme.semibold
                                color: Theme.ink
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }
                            Text {
                                text: (page.mr.source || "") + " → " + (page.mr.target || "")
                                font.family: Theme.mono
                                font.pixelSize: Theme.meta
                                color: Theme.ink2
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }

                            Flow {
                                Layout.fillWidth: true
                                spacing: Theme.s6

                                Badge {
                                    visible: (page.mr.added || 0) > 0
                                    text: "+" + (page.mr.added || 0) + " "
                                          + page.plural(page.mr.added || 0, "record", "records")
                                    kind: "done"
                                }
                                Badge {
                                    visible: (page.mr.edited || 0) > 0
                                    text: (page.mr.edited || 0) + " "
                                          + page.plural(page.mr.edited || 0, "record", "records")
                                          + " edited"
                                    kind: "attention"
                                }
                                Badge {
                                    text: (page.mr.conflicts || 0) === 0
                                          ? "No conflicts"
                                          : ((page.mr.conflicts || 0) + " "
                                             + page.plural(page.mr.conflicts || 0, "conflict", "conflicts"))
                                    kind: (page.mr.conflicts || 0) === 0 ? "neutral" : "attention"
                                }
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.topMargin: Theme.s10
                                implicitHeight: 1
                                color: Theme.lineSoft
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s6
                                Repeater {
                                    model: page.mergeRequest ? (page.mergeRequest.reviewers || []) : []
                                    delegate: RowLayout {
                                        required property var modelData
                                        Layout.fillWidth: true
                                        spacing: Theme.s8
                                        Text {
                                            text: modelData.name || ""
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.body
                                            color: Theme.ink
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        Text {
                                            text: page.reviewerState(modelData.state || "")
                                            font.family: Theme.sans
                                            font.pixelSize: Theme.meta
                                            color: (modelData.state === "approved") ? Theme.accent : Theme.ink3
                                        }
                                    }
                                }
                            }

                            Text {
                                readonly property int needed: page.mr.approvalsNeeded || 0
                                text: (page.mr.target || "main") + " needs " + needed + " "
                                      + page.plural(needed, "approval", "approvals") + " before a merge"
                                font.family: Theme.sans
                                font.pixelSize: Theme.meta
                                color: Theme.ink2
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.s8

                                // Violet primary: merging a branch is a "current /
                                // committed" action, not a green verified one.
                                Rectangle {
                                    id: reviewButton
                                    Layout.fillWidth: true
                                    implicitHeight: Theme.touchTarget
                                    radius: Theme.rControl
                                    color: Theme.violet
                                    opacity: reviewPress.pressed ? 0.88 : 1

                                    function activate() {
                                        // TODO(phase1b): vcs.reviewMerge(mrId).
                                    }

                                    Accessible.role: Accessible.Button
                                    Accessible.name: "Review changes"
                                    Accessible.onPressAction: reviewButton.activate()
                                    activeFocusOnTab: true

                                    Rectangle {
                                        anchors.fill: parent
                                        anchors.margins: -3
                                        radius: Theme.rControl
                                        color: "transparent"
                                        border.width: 2
                                        border.color: Theme.violet
                                        visible: parent.activeFocus
                                    }

                                    Text {
                                        anchors.centerIn: parent
                                        text: "Review changes"
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.body
                                        font.weight: Theme.semibold
                                        color: Theme.textOnAccent
                                    }

                                    MouseArea {
                                        id: reviewPress
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: reviewButton.activate()
                                    }
                                    Keys.onReturnPressed: reviewButton.activate()
                                    Keys.onSpacePressed: reviewButton.activate()
                                }
                                Button {
                                    text: "Comment"
                                    implicitHeight: Theme.touchTarget
                                    Accessible.name: "Comment on the merge request"
                                    onClicked: {
                                        // TODO(phase1b): a comment goes on the merge
                                        // request record; no core method yet.
                                    }
                                }
                            }
                        }
                    }

                    // ---- Branches ----
                    Panel {
                        title: "Branches"
                        Layout.fillWidth: true

                        Repeater {
                            model: page.branches
                            delegate: ColumnLayout {
                                required property var modelData
                                readonly property bool merged: modelData.status === "Merged"
                                readonly property bool protectedBranch: modelData.status === "Protected"

                                Layout.fillWidth: true
                                spacing: Theme.s2

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.s8
                                    Text {
                                        text: modelData.name || ""
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.body
                                        font.weight: Theme.medium
                                        color: merged ? Theme.ink2 : Theme.violet
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                    }
                                    Badge {
                                        visible: protectedBranch
                                        text: modelData.status || ""
                                        kind: "neutral"
                                    }
                                    Text {
                                        visible: !protectedBranch
                                        text: modelData.status || ""
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: merged ? Theme.accent : Theme.ink3
                                    }
                                }
                                Text {
                                    text: modelData.meta || ""
                                    font.family: Theme.sans
                                    font.pixelSize: Theme.meta
                                    color: Theme.ink3
                                    wrapMode: Text.Wrap
                                    Layout.fillWidth: true
                                }
                            }
                        }
                    }

                    // ---- Tags ----
                    Panel {
                        title: "Tags"
                        Layout.fillWidth: true

                        Text {
                            text: "Named points the group can cite and return to"
                            font.family: Theme.sans
                            font.pixelSize: Theme.meta
                            color: Theme.ink3
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.s8

                            Repeater {
                                model: page.tags
                                delegate: RowLayout {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    spacing: Theme.s8
                                    Text {
                                        text: modelData.name || ""
                                        font.family: Theme.mono
                                        font.pixelSize: Theme.body
                                        color: Theme.ink
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        text: modelData.date || ""
                                        font.family: Theme.sans
                                        font.pixelSize: Theme.meta
                                        color: Theme.ink3
                                    }
                                }
                            }
                        }

                        Button {
                            text: "Tag current version"
                            implicitHeight: Theme.touchTarget
                            Layout.fillWidth: true
                            Accessible.name: "Tag current version"
                            onClicked: {
                                // TODO(phase1b): vcs.tag(name, commitId).
                            }
                        }
                    }
                }
            }
        }
    }
}
