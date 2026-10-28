#include "olb_ui_backend.h"

// Generated umbrella: LogosModules (behind modules()) from
// metadata.json#dependencies — typed wrappers + typed event accessors.
#include "logos_sdk.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QJsonParseError>
#include <QUrl>

// A native file dialog hands back a file:// URL; olb_core wants a plain
// absolute path. Accept either so the view can pass what the picker gives it.
static QString toLocalPath(const QString &pathOrUrl)
{
    if (pathOrUrl.startsWith(QStringLiteral("file:")))
        return QUrl(pathOrUrl).toLocalFile();
    return pathOrUrl;
}

void OlbUiBackend::setLastActionResult(QString resultJson)
{
    QJsonParseError err{};
    const QJsonDocument doc = QJsonDocument::fromJson(resultJson.toUtf8(), &err);

    QJsonObject obj;
    if (err.error == QJsonParseError::NoError && doc.isObject())
        obj = doc.object();
    else
        // Not an object, so there is no envelope to extend. Keep the bytes
        // verbatim under a known key rather than dropping a reply we cannot
        // classify.
        obj.insert(QStringLiteral("raw"), resultJson);

    obj.insert(QStringLiteral("seq"), QJsonValue(qint64(++m_actionSeq)));

    OlbUiSimpleSource::setLastActionResult(
        QString::fromUtf8(QJsonDocument(obj).toJson(QJsonDocument::Compact)));
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
    setLockState(modules().olb_core.lock_state());
    setMyId(modules().olb_core.my_olb_id());
    setInvites(modules().olb_core.invites());
}

void OlbUiBackend::refreshLockState()
{
    setLockState(modules().olb_core.lock_state());
}

void OlbUiBackend::createPassphrase(QString passphrase)
{
    setLastActionResult(modules().olb_core.create_passphrase(passphrase));
    // refreshBoard() carries lockState in its snapshot, so asking for it
    // again here would be one more call that can hang for nothing.
    refreshBoard();
}

void OlbUiBackend::unlockStore(QString passphrase)
{
    setLastActionResult(modules().olb_core.unlock(passphrase));
    // Unlocking is the moment everything becomes readable at once, and the
    // snapshot includes the new lock state.
    refreshBoard();
}

void OlbUiBackend::refreshIdentity()
{
    setMyId(modules().olb_core.my_olb_id());
    setIdentity(modules().olb_core.identity_fingerprint());
    setInvites(modules().olb_core.invites());
}

void OlbUiBackend::inviteMember(QString projectId, QString olbId, QString role)
{
    setLastActionResult(modules().olb_core.invite_member(projectId, olbId, role));
}

void OlbUiBackend::acceptInvite(QString inviteId)
{
    setLastActionResult(modules().olb_core.accept_invite(inviteId));
    // Accepting installs a key and joins a project, so everything the
    // screens show can change at once.
    refreshBoard();
}

/// Re-serialise one section of a snapshot back to the compact JSON string its
/// PROP carries. Absent or null sections become "", which every screen
/// already treats as "nothing yet".
static QString section(const QJsonObject &snap, const char *key)
{
    const QJsonValue v = snap.value(QLatin1String(key));
    if (v.isUndefined() || v.isNull())
        return QString();
    if (v.isObject())
        return QString::fromUtf8(QJsonDocument(v.toObject()).toJson(QJsonDocument::Compact));
    if (v.isArray())
        return QString::fromUtf8(QJsonDocument(v.toArray()).toJson(QJsonDocument::Compact));
    return v.toVariant().toString();
}

void OlbUiBackend::refreshBoard()
{
    // ONE synchronous round-trip, not eight.
    //
    // Every call across this seam can block the UI thread indefinitely: the
    // 20 s Timeout in logos_mode.h does not fire from inside a QtRO handler,
    // because the call is re-entrant on the event loop it needs in order to
    // receive its own reply (docs/08 R21). On 28 Oct that froze the whole
    // app for over six minutes against an idle olb_core. The number of calls
    // per refresh is therefore the number of chances to freeze, and this was
    // the hottest path in the app — every store change runs it.
    //
    // The PROPs are unchanged, so no QML moves; only the fetching collapses.
    const QString snapJson = modules().olb_core.snapshot();

    QJsonParseError err{};
    const QJsonDocument doc = QJsonDocument::fromJson(snapJson.toUtf8(), &err);
    if (err.error != QJsonParseError::NoError || !doc.isObject()) {
        // A snapshot we cannot read must not wipe every screen to blank.
        // Leave the PROPs holding the last good values and say so.
        setLastActionResult(QStringLiteral(
            "{\"ok\":false,\"error\":{\"code\":\"internal\","
            "\"message\":\"the board could not be read; showing the last "
            "known state\"}}"));
        return;
    }
    const QJsonObject snap = doc.object();

    setBoard(section(snap, "board"));
    setNotes(section(snap, "notes"));
    setProjects(section(snap, "projects"));
    setProfile(section(snap, "profile"));
    setHistory(section(snap, "history"));
    setMembers(section(snap, "members"));
    setInvites(section(snap, "invites"));
    setLockState(section(snap, "lockState"));
    setFixtures(snap.value(QStringLiteral("fixtures")).toBool(false));
}

void OlbUiBackend::openRecord(QString recordId)
{
    setOpenedRecord(modules().olb_core.record(recordId));
}

void OlbUiBackend::createProject(QString specJson)
{
    // The result goes out FIRST, before refreshing anything.
    //
    // Every call in refreshBoard() is a synchronous IPC round-trip, and on
    // 28 Oct one of them (set_current_project) was observed blocking the UI
    // thread for over six minutes against an idle olb_core — a lost reply,
    // not slow work. Publishing the result after them means a hung refresh
    // also costs the person any word of an action that already succeeded:
    // the project was on disk while the button still read "Creating...".
    // The board is reactive, so arriving a moment before it repopulates is
    // strictly better than arriving never.
    setLastActionResult(modules().olb_core.create_project(specJson));
    refreshBoard();
}

void OlbUiBackend::createIdea(QString projectId, QString title, QString summary)
{
    setLastActionResult(modules().olb_core.create_idea(projectId, title, summary));
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
