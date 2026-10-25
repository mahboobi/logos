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
 * the auto-synced PROPs; this backend calls olb_core via modules().
 * See CLAUDE.md "Hard constraints" R2 and docs/08 R2.
 */
class OlbUiBackend : public OlbUiSimpleSource,
                     public LogosUiPluginContext
{
public:
    void refreshSessionStatus() override;
};
