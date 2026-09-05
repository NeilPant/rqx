package com.rqx

import com.intellij.openapi.options.BoundConfigurable
import com.intellij.openapi.project.ProjectManager
import com.intellij.openapi.ui.DialogPanel
import com.intellij.ui.dsl.builder.bindSelected
import com.intellij.ui.dsl.builder.bindText
import com.intellij.ui.dsl.builder.panel
import com.redhat.devtools.lsp4ij.LanguageServerManager

/**
 * Settings → Tools → rqx. The README, the tool window's own error message and
 * `rqx setup jetbrains` have all pointed here since the plugin was written, and until
 * this existed the only way to change any of it was to edit `rqx.xml` by hand.
 */
class RqxConfigurable : BoundConfigurable("rqx") {
    override fun createPanel(): DialogPanel {
        val state = RqxSettings.instance.state
        return panel {
            row("Binary:") {
                // The getter/setter pair rather than a property reference: the state
                // stores these as nullable strings and the DSL binds a non-null one.
                textField()
                    .bindText({ state.path.orEmpty() }, { state.path = it })
                    .comment("Leave empty to use the first <code>rqx</code> on your PATH.")
            }
            row("Review against:") {
                textField()
                    .bindText({ state.base ?: "origin/HEAD" }, { state.base = it })
                    .comment("The revision <b>Review the Diff</b> and <b>Verify</b> compare against.")
            }
            group("What the server does") {
                row {
                    checkBox("Scan the workspace when the server starts")
                        .bindSelected(state::scanOnStartup)
                }
                row {
                    checkBox("Rescan after every save")
                        .bindSelected(state::scanOnSave)
                        .comment(
                            "The cross-file rules can only change when some <i>other</i> file " +
                                "does. Turn this off on a very large repository."
                        )
                }
                row {
                    checkBox("Show each file's grade as a code lens").bindSelected(state::codeLens)
                }
                row {
                    checkBox("Report half-applied design patterns")
                        .bindSelected(state::patterns)
                        .comment("They never move the health score.")
                }
            }
        }
    }

    /**
     * The server reads these once, at the handshake, so a changed setting needs a new
     * process rather than a notification.
     */
    override fun apply() {
        super.apply()
        for (project in ProjectManager.getInstance().openProjects) {
            val servers = LanguageServerManager.getInstance(project)
            servers.stop("rqx", LanguageServerManager.StopOptions().setWillDisable(false))
            servers.start("rqx")
        }
    }
}
