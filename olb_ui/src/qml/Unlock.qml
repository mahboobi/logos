import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The passphrase gate. Shown instead of the app whenever the store is
// sealed — there is nothing to render behind it, because nothing is
// readable yet.
//
// Two states, one screen: `needs_setup` on a store that has no passphrase,
// `locked` on one that does. They differ in copy and in whether a
// confirmation field is shown; the rest is identical, so they share a
// component rather than drifting apart as two.
Item {
    id: screen
    property var lockState: ({})
    property string lastResult: ""
    signal createPassphrase(string passphrase)
    signal unlockStore(string passphrase)

    readonly property string state_: lockState.state || "needs_setup"
    readonly property bool setup: state_ === "needs_setup"
    readonly property int minLength: lockState.minLength || 10
    property bool busy: false

    // The core returns {"ok":false,"error":{code,message}} on a refusal.
    readonly property string failure: {
        if (!screen.lastResult) return "";
        try {
            const r = JSON.parse(screen.lastResult);
            if (r && r.ok === false && r.error) return r.error.message || "";
        } catch (e) {}
        return "";
    }

    readonly property bool longEnough: pass.text.length >= screen.minLength
    readonly property bool matches: !screen.setup || confirm.text === pass.text
    readonly property bool ready: screen.longEnough && screen.matches && !screen.busy

    function submit() {
        if (!screen.ready) return;
        screen.busy = true;
        if (screen.setup) screen.createPassphrase(pass.text);
        else screen.unlockStore(pass.text);
    }

    // A result arriving means the attempt finished, either way.
    onLastResultChanged: screen.busy = false

    Rectangle {
        anchors.fill: parent
        color: Theme.ground
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.s32 * 2, 520)
        spacing: Theme.s20

        Text {
            text: screen.setup ? "Set a passphrase" : "Unlock your notebook"
            font.family: Theme.sans
            font.pixelSize: Theme.pageTitle
            font.weight: Theme.semibold
            font.letterSpacing: Theme.titleSpacing
            color: Theme.ink
        }

        Text {
            text: screen.setup
                  ? "Everything you record is encrypted with this passphrase — records, "
                    + "files, keys and your identity. It is never sent anywhere."
                  : "Your records are encrypted on this computer. Nothing can be read "
                    + "until you unlock them."
            font.family: Theme.sans
            font.pixelSize: Theme.lede
            color: Theme.ink2
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        // The single most important thing on this screen. It is amber
        // rather than grey because it is a warning, and it is stated before
        // the field rather than under it, because afterwards is too late.
        Rectangle {
            visible: screen.setup
            Layout.fillWidth: true
            Layout.preferredHeight: warn.implicitHeight + Theme.s16 * 2
            color: Theme.amberTint
            radius: Theme.rControl

            Text {
                id: warn
                anchors.fill: parent
                anchors.margins: Theme.s16
                text: "There is no way to reset this. If you forget it, the notebook "
                      + "cannot be recovered by us or by anyone else. Write it down "
                      + "somewhere safe."
                font.family: Theme.sans
                font.pixelSize: Theme.body
                font.weight: Theme.medium
                color: Theme.amber
                wrapMode: Text.Wrap
            }
        }

        FieldLabel { text: screen.setup ? "New passphrase" : "Passphrase" }
        TextField {
            id: pass
            Layout.fillWidth: true
            implicitHeight: Theme.touchTarget
            echoMode: TextInput.Password
            font.family: Theme.sans
            font.pixelSize: Theme.body
            enabled: !screen.busy
            Accessible.name: screen.setup ? "New passphrase" : "Passphrase"
            onAccepted: screen.submit()
            focus: true
        }

        Text {
            visible: screen.setup
            text: pass.text.length === 0
                  ? "At least " + screen.minLength + " characters. A few ordinary words "
                    + "you will remember beats a short complicated one."
                  : (screen.longEnough
                     ? "Long enough."
                     : "Needs " + (screen.minLength - pass.text.length) + " more character"
                       + ((screen.minLength - pass.text.length) === 1 ? "" : "s") + ".")
            font.family: Theme.sans
            font.pixelSize: Theme.small
            color: pass.text.length === 0 ? Theme.ink3
                                          : (screen.longEnough ? Theme.accent : Theme.ink3)
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        FieldLabel { visible: screen.setup; text: "Type it again" }
        TextField {
            id: confirm
            visible: screen.setup
            Layout.fillWidth: true
            implicitHeight: Theme.touchTarget
            echoMode: TextInput.Password
            font.family: Theme.sans
            font.pixelSize: Theme.body
            enabled: !screen.busy
            Accessible.name: "Confirm the passphrase"
            onAccepted: screen.submit()
        }

        Text {
            visible: screen.setup && confirm.text.length > 0 && !screen.matches
            text: "These do not match."
            font.family: Theme.sans
            font.pixelSize: Theme.small
            color: Theme.amber
            Layout.fillWidth: true
        }

        Text {
            visible: screen.failure !== ""
            text: screen.failure
            font.family: Theme.sans
            font.pixelSize: Theme.body
            color: Theme.amber
            wrapMode: Text.Wrap
            Layout.fillWidth: true
            Accessible.name: "Error: " + screen.failure
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s12

            PrimaryButton {
                text: screen.busy
                      ? "Working…"
                      : (screen.setup ? "Set passphrase and continue" : "Unlock")
                accessibleName: screen.setup ? "Set passphrase and continue" : "Unlock"
                enabled: screen.ready
                onClicked: screen.submit()
            }

            Text {
                // Argon2id is deliberately slow. Saying so stops a two-second
                // pause reading as a hang.
                visible: screen.busy
                text: "Deriving the key — this takes a moment by design."
                font.family: Theme.sans
                font.pixelSize: Theme.small
                color: Theme.ink3
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
        }
    }
}
