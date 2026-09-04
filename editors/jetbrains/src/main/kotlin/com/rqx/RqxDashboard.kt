package com.rqx

import com.intellij.openapi.Disposable
import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory
import com.intellij.ui.jcef.JBCefApp
import com.intellij.ui.jcef.JBCefBrowser
import java.io.BufferedReader
import javax.swing.JLabel
import javax.swing.SwingConstants

/**
 * The dashboard is one page, served by `rqx serve`. Hosting it here rather than
 * rebuilding it in Swing is what keeps the IDE and the browser showing the same thing:
 * a panel written twice is a panel that disagrees with itself by the second release.
 */
class RqxDashboard : ToolWindowFactory, Disposable {
    private var server: Process? = null

    override fun createToolWindowContent(project: Project, window: ToolWindow) {
        val content = ContentFactory.getInstance()
        val panel = when {
            !JBCefApp.isSupported() -> JLabel(
                "<html>This IDE has no embedded browser.<br>" +
                    "Run <code>rqx serve --open</code> to see the dashboard.</html>",
                SwingConstants.CENTER
            )
            else -> browser(project)?.component
                ?: JLabel("rqx could not start. Check Settings → Tools → rqx.", SwingConstants.CENTER)
        }
        window.contentManager.addContent(content.createContent(panel, "", false))
    }

    /** Starts the server and reads back the address it bound, which it prints first. */
    private fun browser(project: Project): JBCefBrowser? {
        val binary = RqxBinary.locate(project)
        val root = project.basePath ?: return null
        val process = ProcessBuilder(binary.toString(), "serve", root, "--addr", "127.0.0.1:0")
            .redirectErrorStream(false)
            .start()
        server = process

        val line = process.inputStream.bufferedReader().use(BufferedReader::readLine) ?: return null
        val url = line.split(' ').firstOrNull { it.startsWith("http://") } ?: return null
        return JBCefBrowser(url)
    }

    // The server outlives a closed tool window only until the IDE lets go of this.
    override fun dispose() {
        server?.destroy()
        server = null
    }
}
