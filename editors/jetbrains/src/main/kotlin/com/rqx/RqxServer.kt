package com.rqx

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider

/**
 * Starts `rqx lsp` in the project directory. Everything else the plugin shows comes
 * back over the protocol, so a capability added to the server appears here without a
 * release of this.
 */
class RqxServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project) =
        RqxConnection(project)

    override fun createLanguageClient(project: Project) =
        LanguageClientImpl(project)
}

class RqxConnection(private val project: Project) : OSProcessStreamConnectionProvider() {
    init {
        val binary = RqxBinary.locate(project)
        commandLine = GeneralCommandLine(binary.toString(), "lsp").apply {
            // The server resolves RQX.toml from where it is started, so a monorepo
            // package with its own config is read rather than the repository root's.
            project.basePath?.let { withWorkDirectory(it) }
        }
    }

    override fun getInitializationOptions(rootUri: VirtualFile?): Any =
        mapOf(
            "scanOnStartup" to RqxSettings.instance.scanOnStartup,
            "scanOnSave" to RqxSettings.instance.scanOnSave,
            "codeLens" to RqxSettings.instance.codeLens,
            "patterns" to RqxSettings.instance.patterns,
        )
}
