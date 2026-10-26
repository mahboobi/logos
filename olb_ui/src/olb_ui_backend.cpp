#include "olb_ui_backend.h"

// Generated umbrella: LogosModules (behind modules()) from
// metadata.json#dependencies — typed wrappers + typed event accessors.
#include "logos_sdk.h"

#include <QUrl>

// A native file dialog hands back a file:// URL; olb_core wants a plain
// absolute path. Accept either so the view can pass what the picker gives it.
static QString toLocalPath(const QString &pathOrUrl)
{
    if (pathOrUrl.startsWith(QStringLiteral("file:")))
        return QUrl(pathOrUrl).toLocalFile();
    return pathOrUrl;
}

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
    setBoard(modules().olb_core.board());
    setFixtures(modules().olb_core.fixtures_enabled());
}

void OlbUiBackend::refreshBoard()
{
    setBoard(modules().olb_core.board());
    setFixtures(modules().olb_core.fixtures_enabled());
    setNotes(modules().olb_core.notes());
    setProjects(modules().olb_core.projects());
    setProfile(modules().olb_core.profile());
    setHistory(modules().olb_core.history());
    setMembers(modules().olb_core.members());
}

void OlbUiBackend::openRecord(QString recordId)
{
    setOpenedRecord(modules().olb_core.record(recordId));
}

void OlbUiBackend::createProject(QString specJson)
{
    setLastActionResult(modules().olb_core.create_project(specJson));
    refreshBoard();
}

void OlbUiBackend::setCurrentProject(QString projectId)
{
    setLastActionResult(modules().olb_core.set_current_project(projectId));
    refreshBoard();
}

void OlbUiBackend::createNote(QString title, QString body)
{
    setLastActionResult(modules().olb_core.create_note(title, body));
    refreshBoard();
}

void OlbUiBackend::promoteNote(QString noteId, QString projectId, QString stage)
{
    setLastActionResult(modules().olb_core.promote_note(noteId, projectId, stage));
    refreshBoard();
}

void OlbUiBackend::saveProfile(QString name, QString orcid)
{
    setLastActionResult(modules().olb_core.set_profile(name, orcid));
    refreshBoard();
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
    setLastStorageResult(modules().olb_core.upload_file(toLocalPath(absPath)));
}

void OlbUiBackend::fetchFile(QString cid, QString absDest)
{
    setLastStorageResult(modules().olb_core.fetch_file(cid, toLocalPath(absDest)));
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

    // Any store change (project, note, profile) refreshes every screen.
    modules().olb_core.onStore_changed([this](QString) {
        refreshBoard();
    });

    modules().olb_core.onFile_fetched([this](QString result) {
        setLastStorageResult(result);
        setCids(modules().olb_core.stored_cids());
    });

    // Populate the view without waiting for the first interaction.
    syncFromCore();
}
