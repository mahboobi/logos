#include "olb_ui_backend.h"

// Generated umbrella: LogosModules (behind modules()) from
// metadata.json#dependencies — typed wrappers + typed event accessors.
#include "logos_sdk.h"

void OlbUiBackend::refreshSessionStatus()
{
    // Typed, synchronous call across the IPC seam. olb_core's LIDL method is
    // session_status() -> tstr; the dep wrapper is Qt-typed, so it lands as a
    // QString. snake_case is preserved by the generator.
    const QString status = modules().olb_core.session_status();

    // PROPs from the .rep — QtRO pushes every setter to the QML replica.
    setCoreStatus(status);
    setCoreReachable(!status.isEmpty());
}

void OlbUiBackend::onContextReady()
{
    // Typed module-event subscription feeding a .rep PROP. The Rust events
    // trait declares session_changed(status), which surfaces here as
    // onSession_changed.
    modules().olb_core.onSession_changed([this](QString status) {
        setCoreStatus(status);
        setCoreReachable(true);
    });

    // Populate the view without waiting for the first button press.
    refreshSessionStatus();
}
