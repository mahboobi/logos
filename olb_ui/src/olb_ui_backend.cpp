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
    setIdentity(modules().olb_core.identity_fingerprint());
    setRecords(modules().olb_core.received_records());
}

void OlbUiBackend::publishTestRecord(QString title)
{
    // olb_core signs the record and hands it to delivery_module. The peer's
    // copy arrives back through the record_received subscription below, not
    // from this return value.
    modules().olb_core.publish_test_record(title);
    setRecords(modules().olb_core.received_records());
}

void OlbUiBackend::onContextReady()
{
    // Typed module-event subscriptions feeding .rep PROPs. The Rust events
    // trait declares session_changed(status) and record_received(record_json),
    // which surface here as onSession_changed / onRecord_received.
    modules().olb_core.onSession_changed([this](QString status) {
        setCoreStatus(status);
        setCoreReachable(true);
    });

    modules().olb_core.onRecord_received([this](QString) {
        // Pull the whole list rather than append: olb_core owns ordering and
        // dedup by messageHash.
        setRecords(modules().olb_core.received_records());
        setCoreStatus(modules().olb_core.session_status());
    });

    // Populate the view without waiting for the first button press.
    refreshSessionStatus();
}
