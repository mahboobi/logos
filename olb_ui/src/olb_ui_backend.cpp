#include "olb_ui_backend.h"

// Generated umbrella: LogosModules (behind modules()) from
// metadata.json#dependencies — typed wrappers + typed event accessors.
#include "logos_sdk.h"

void OlbUiBackend::syncFromCore()
{
    // Typed, synchronous calls across the IPC seam. olb_core's LIDL methods
    // return tstr; the dep wrappers are Qt-typed, so they land as QString.
    // snake_case is preserved by the generator.
    const QString status = modules().olb_core.session_status();
    setCoreStatus(status);
    setCoreReachable(!status.isEmpty());
    setIdentity(modules().olb_core.identity_fingerprint());
    setRecords(modules().olb_core.received_records());
    setCids(modules().olb_core.stored_cids());
}

void OlbUiBackend::refreshSessionStatus()
{
    syncFromCore();
}

void OlbUiBackend::publishTestRecord(QString title)
{
    // olb_core signs the record and hands it to delivery_module. A peer's copy
    // arrives through the record_received subscription, not this return value.
    modules().olb_core.publish_test_record(title);
    syncFromCore();
}

void OlbUiBackend::uploadFile(QString absPath)
{
    // Asynchronous: this returns a session envelope, and the CID arrives on
    // file_stored. Surface the immediate envelope so a rejected path (not
    // absolute, missing) is visible at once rather than silently doing nothing.
    setLastStorageResult(modules().olb_core.upload_file(absPath));
}

void OlbUiBackend::fetchFile(QString cid, QString absDest)
{
    setLastStorageResult(modules().olb_core.fetch_file(cid, absDest));
}

void OlbUiBackend::onContextReady()
{
    // Typed module-event subscriptions feeding .rep PROPs. The Rust events
    // trait declares session_changed / record_received / file_stored /
    // file_fetched, which surface here with the on<Name> spelling.
    modules().olb_core.onSession_changed([this](QString status) {
        setCoreStatus(status);
        setCoreReachable(true);
    });

    modules().olb_core.onRecord_received([this](QString) {
        setRecords(modules().olb_core.received_records());
        setCoreStatus(modules().olb_core.session_status());
    });

    modules().olb_core.onFile_stored([this](QString result) {
        setLastStorageResult(result);
        setCids(modules().olb_core.stored_cids());
        setCoreStatus(modules().olb_core.session_status());
    });

    modules().olb_core.onFile_fetched([this](QString result) {
        setLastStorageResult(result);
        setCids(modules().olb_core.stored_cids());
    });

    // Populate the view without waiting for the first interaction.
    syncFromCore();
}
