#pragma once

#include "rep_olb_ui_source.h"
#include "logos_ui_plugin_context.h"

/**
 * @brief Open Labbook UI backend (universal authoring model).
 *
 * The QML view is sandboxed and cannot reach olb_core directly — there is no
 * logos.callModule on this path. The chain is:
 *
 *   QML --QtRO--> OlbUiBackend --LogosAPI--> olb_core
 *
 * QML calls a .rep SLOT on logos.module("olb_ui") and reads replies through
 * the auto-synced PROPs; this backend calls olb_core through modules().
 * See CLAUDE.md "Hard constraints" R2 and docs/08 R2.
 */
class OlbUiBackend : public OlbUiSimpleSource,
                     public LogosUiPluginContext
{
public:
    void refreshSessionStatus() override;
    void publishTestRecord(QString title) override;
    void uploadFile(QString absPath) override;
    void fetchFile(QString cid, QString absDest) override;
    void refreshBoard() override;
    void refreshIdentity() override;
    void refreshLockState() override;
    void createPassphrase(QString passphrase) override;
    void unlockStore(QString passphrase) override;
    void inviteMember(QString projectId, QString olbId, QString role) override;
    void acceptInvite(QString inviteId) override;
    void createProject(QString specJson) override;
    void setCurrentProject(QString projectId) override;
    void createNote(QString title, QString body) override;
    void promoteNote(QString noteId, QString projectId, QString stage) override;
    void saveProfile(QString name, QString orcid) override;
    void openRecord(QString recordId) override;

    /// Stamp every action result with a monotonic sequence number.
    ///
    /// repc's generated setter only emits a change signal when the new value
    /// differs from the old one. Two identical results in a row therefore
    /// emit nothing — and that is the common case, not a corner: retry a
    /// create with the same name, or mistype the same passphrase twice, and
    /// the core returns a byte-identical error envelope. The screen is still
    /// waiting on a reply that already arrived, so it sits on "Creating..."
    /// or "Working..." until the app is restarted.
    ///
    /// The sequence goes inside the JSON rather than into a PROP of its own
    /// so that a result and its freshness cannot arrive out of order.
    void setLastActionResult(QString lastActionResult) override;

    /// Fires when ui-host hands the plugin its LogosAPI, so subscriptions are
    /// live before the view makes its first call.
    void onContextReady() override;

private:
    /// Pull olb_core's current view into the PROPs. olb_core owns ordering and
    /// dedup, so the UI re-reads rather than maintaining its own copy.
    void syncFromCore();

    /// Counts completed actions. Only ever used to make each result distinct.
    quint64 m_actionSeq = 0;
};
