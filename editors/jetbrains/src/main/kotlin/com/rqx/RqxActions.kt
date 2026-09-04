package com.rqx

import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerManager
import com.redhat.devtools.lsp4ij.commands.CommandExecutor
import com.redhat.devtools.lsp4ij.commands.LSPCommandContext
import org.eclipse.lsp4j.Command

/**
 * Every action is one `workspace/executeCommand`. The server decides what each one
 * means, so this file states the names and nothing else — a command added there needs
 * one line here rather than an implementation.
 */
abstract class RqxCommand(private val command: String, private val label: String) : AnAction() {
    override fun actionPerformed(event: AnActionEvent) {
        val project = event.project ?: return
        CommandExecutor.executeCommand(
            LSPCommandContext(Command(label, command, emptyList()), project)
        )
    }
}

class ScanWorkspace : RqxCommand("rqx.scan", "Scan the workspace")
class CheckDiff : RqxCommand("rqx.check", "Review the diff")
class StartHere : RqxCommand("rqx.startHere", "Open what matters first")
class ShowCatalogue : RqxCommand("rqx.catalogue", "Show the rule catalogue")
class TogglePatterns : RqxCommand("rqx.togglePatterns", "Toggle pattern advice")
class ToggleCodeLens : RqxCommand("rqx.toggleCodeLens", "Toggle the code lens")

class RestartServer : AnAction() {
    override fun actionPerformed(event: AnActionEvent) {
        val project: Project = event.project ?: return
        LanguageServerManager.getInstance(project)
            .stop("rqx", LanguageServerManager.StopOptions().setWillDisable(false))
        LanguageServerManager.getInstance(project).start("rqx")
    }
}
