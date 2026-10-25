#include "olb_ui_backend.h"

// Generated umbrella: LogosModules (behind modules()) built from
// metadata.json#dependencies — typed wrappers + typed event accessors.
#include "logos_sdk.h"

void OlbUiBackend::refreshSessionStatus()
{
    // TODO(phase0-task3): call modules().olb_core.session_status() here. The
    // exact generated caller signature (sync vs callback) is read off the
    // generated logos_sdk.h after the first build rather than guessed —
    // CLAUDE.md forbids inventing Logos APIs.
    setCoreReachable(false);
    setCoreStatus(QStringLiteral("{\"state\":\"backend-reached-core-call-not-wired\"}"));
}
