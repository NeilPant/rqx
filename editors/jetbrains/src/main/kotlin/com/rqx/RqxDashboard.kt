package com.rqx

import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.components.JBLabel
import com.intellij.ui.content.ContentFactory
import com.intellij.ui.jcef.JBCefApp
import com.intellij.ui.jcef.JBCefBrowser
import com.redhat.devtools.lsp4ij.LanguageServerManager
import org.eclipse.lsp4j.ExecuteCommandParams
import java.awt.BorderLayout
import javax.swing.JPanel
import javax.swing.SwingConstants

/**
 * The dashboard is one page, served by `rqx serve`. Hosting it here rather than
 * rebuilding it in Swing is what keeps the IDE and the browser showing the same thing:
 * a panel written twice is a panel that disagrees with itself by the second release.
 *
 * The server starts it and owns it, so a second editor window shows the same page on
 * the same port rather than starting a server of its own.
 */
class RqxDashboard : ToolWindowFactory, Disposable {
    private var browser: JBCefBrowser? = null

    override fun createToolWindowContent(project: Project, window: ToolWindow) {
        val panel = JPanel(BorderLayout())
        val waiting = JBLabel(
            "<html>Starting the dashboard…<br>" +
                "The first run waits for its own scan of this project.</html>",
            SwingConstants.CENTER,
        )
        panel.add(waiting, BorderLayout.CENTER)
        window.contentManager.addContent(
            ContentFactory.getInstance().createContent(panel, "", false)
        )

        if (!JBCefApp.isSupported()) {
            waiting.text =
                "<html>This IDE has no embedded browser.<br>" +
                    "Run <code>rqx serve --open</code> to see the dashboard.</html>"
            return
        }
        // The command answers only once the page's server is listening, so asking on
        // the UI thread would freeze the IDE for the length of a scan.
        askForUrl(project) { url, failure ->
            ApplicationManager.getApplication().invokeLater {
                if (url == null) {
                    waiting.text = "<html>The dashboard did not start.<br>$failure</html>"
                    return@invokeLater
                }
                val embedded = JBCefBrowser(url)
                browser = embedded
                panel.remove(waiting)
                panel.add(embedded.component, BorderLayout.CENTER)
                panel.revalidate()
                panel.repaint()
            }
        }
    }

    /**
     * `rqx.dashboard` answers with the URL of the one server this session owns.
     * lsp4j is asked directly rather than through LSP4IJ's command executor, because
     * the executor runs a command for its side effect and discards what it returned.
     */
    private fun askForUrl(project: Project, answer: (String?, String) -> Unit) {
        LanguageServerManager.getInstance(project)
            .getLanguageServer("rqx")
            .whenComplete { item, error ->
                val workspace = item?.server?.workspaceService
                if (workspace == null) {
                    answer(null, error?.message ?: "the rqx language server is not running")
                    return@whenComplete
                }
                workspace
                    .executeCommand(ExecuteCommandParams("rqx.dashboard", emptyList<Any>()))
                    .whenComplete { result, failure ->
                        if (failure != null) {
                            answer(null, failure.message.orEmpty())
                            return@whenComplete
                        }
                        val fields = result as? Map<*, *>
                        answer(
                            fields?.get("url") as? String,
                            fields?.get("error") as? String ?: "the server named no address",
                        )
                    }
            }
    }

    // The language server owns the page's process and stops it on shutdown, so there
    // is nothing here to kill — only the browser to let go of.
    override fun dispose() {
        browser?.let(com.intellij.openapi.util.Disposer::dispose)
        browser = null
    }
}
