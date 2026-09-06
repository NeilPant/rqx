"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.LanguageServer = void 0;
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
const node_path_1 = require("node:path");
const binary_1 = require("./binary");
const LANGUAGES = ["typescript", "typescriptreact", "python", "go", "java", "cpp"];
/// Settings the server reads at startup. Changing one of these needs a new client,
/// because `initializationOptions` are captured when the client is constructed.
const RESTARTS = ["path", "configFile", "scanOnStartup", "scanOnSave", "codeLens", "patterns"];
function settings() {
    return vscode_1.workspace.getConfiguration("rqx");
}
// The server is a child process, so a relative path would resolve against whatever
// directory the editor was launched from rather than against the project.
function resolveInWorkspace(path) {
    const root = vscode_1.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!root || (0, node_path_1.isAbsolute)(path)) {
        return path;
    }
    return (0, node_path_1.join)(root, path);
}
function initializationOptions() {
    const configured = settings().get("configFile", "").trim();
    return {
        configFile: configured ? resolveInWorkspace(configured) : undefined,
        scanOnStartup: settings().get("scanOnStartup", true),
        scanOnSave: settings().get("scanOnSave", true),
        codeLens: settings().get("codeLens", false),
        patterns: settings().get("patterns", false),
    };
}
/// `vscode.DiagnosticSeverity` counts up as severity falls — Error 0 through Hint 3 —
/// so the cap is a floor on the number.
function quieten(diagnostic, cap) {
    const severity = Math.max(diagnostic.severity, cap);
    if (severity === diagnostic.severity) {
        return diagnostic;
    }
    const quiet = new vscode_1.Diagnostic(diagnostic.range, diagnostic.message, severity);
    quiet.code = diagnostic.code;
    quiet.source = diagnostic.source;
    quiet.relatedInformation = diagnostic.relatedInformation;
    quiet.tags = diagnostic.tags;
    // Everything the views read travels on the data, and dropping it here would empty
    // them the moment somebody quietened the Problems panel.
    quiet.data = diagnostic.data;
    return quiet;
}
/// rqx is a second opinion alongside the language's own server, and on an existing
/// codebase its first run is long. Capping the severity lets someone keep working
/// through the list without the Problems panel drowning what their compiler said.
function severityCap() {
    switch (settings().get("severity", "asReported")) {
        case "warning":
            return vscode_1.DiagnosticSeverity.Warning;
        case "information":
            return vscode_1.DiagnosticSeverity.Information;
        case "hint":
            return vscode_1.DiagnosticSeverity.Hint;
        default:
            return undefined;
    }
}
function serverOptions(command) {
    const cwd = vscode_1.workspace.workspaceFolders?.[0]?.uri.fsPath;
    const run = {
        command,
        args: ["lsp"],
        options: cwd ? { cwd } : undefined,
    };
    return { run, debug: run };
}
/// The connection to `rqx lsp`: starting it, restarting it when a setting the server
/// reads changes, and asking it questions.
class LanguageServer {
    context;
    log;
    index;
    events;
    client;
    binary;
    constructor(context, log, index, events) {
        this.context = context;
        this.log = log;
        this.index = index;
        this.events = events;
    }
    get running() {
        return this.client !== undefined;
    }
    /// Where the binary the server is running from lives, for the commands that shell
    /// out to it rather than asking over the protocol.
    get executable() {
        return this.binary;
    }
    async start() {
        const binary = await (0, binary_1.resolveBinary)(this.context, this.log);
        if (!binary) {
            this.events.onStateChange();
            await this.offerRecovery();
            return;
        }
        this.binary = binary;
        const clientOptions = {
            documentSelector: LANGUAGES.map((language) => ({ scheme: "file", language })),
            outputChannel: this.log,
            initializationOptions: initializationOptions(),
            synchronize: {
                fileEvents: vscode_1.workspace.createFileSystemWatcher("**/RQX.toml"),
            },
            middleware: {
                // Applied on the way in rather than asked of the server: the severity a rule
                // declares is part of the catalogue and the same in CI, so this is the client
                // choosing how loudly to render it, not rqx changing its mind.
                handleDiagnostics: (uri, diagnostics, next) => {
                    this.index.absorb(uri, diagnostics);
                    const cap = severityCap();
                    next(uri, cap === undefined ? diagnostics : diagnostics.map((one) => quieten(one, cap)));
                },
            },
        };
        const client = new node_1.LanguageClient("rqx", "Rqx", serverOptions(binary), clientOptions);
        client.onNotification("rqx/health", (health) => this.events.onHealth(health));
        client.onNotification("rqx/report", (report) => {
            this.index.setReport(report);
            this.events.onReport(report);
        });
        try {
            await client.start();
            this.client = client;
        }
        catch (error) {
            this.client = undefined;
            this.log.appendLine(`the server did not start: ${String(error)}`);
            this.events.onStateChange();
            const choice = await vscode_1.window.showErrorMessage(`Rqx: \`${binary} lsp\` did not start. See the log for what it said.`, "Show Log", "Retry");
            if (choice === "Show Log") {
                this.log.show();
            }
            else if (choice === "Retry") {
                await this.start();
            }
            return;
        }
        await (0, binary_1.warnOnVersionMismatch)(client.initializeResult?.serverInfo?.version, this.log);
        this.events.onStateChange();
        const catalogue = await this.send("rqx.catalogue");
        if (catalogue) {
            this.index.setCatalogue(catalogue);
        }
    }
    async offerRecovery() {
        const choice = await vscode_1.window.showErrorMessage('Rqx could not find or download a binary. Retry if you were offline, install it with `cargo install rqx`, or point "rqx.path" at one.', "Retry", "Open Settings", "Show Log");
        if (choice === "Retry") {
            await this.start();
        }
        else if (choice === "Open Settings") {
            await vscode_1.commands.executeCommand("workbench.action.openSettings", "rqx.path");
        }
        else if (choice === "Show Log") {
            this.log.show();
        }
    }
    /// Runs with no client too, so pointing `rqx.path` at a real binary recovers a
    /// session that started without one instead of needing the window reloaded.
    async restart(reason) {
        this.log.appendLine(`restarting: ${reason}`);
        // A new client rather than `restart()`, because the options are captured when the
        // client is constructed and a restart would send the old ones again.
        await this.client?.stop();
        this.client = undefined;
        await this.start();
    }
    restartsFor(affects) {
        return RESTARTS.map((setting) => `rqx.${setting}`).find((section) => affects(section));
    }
    async send(method, ...args) {
        if (!this.client) {
            return undefined;
        }
        try {
            return await this.client.sendRequest("workspace/executeCommand", {
                command: method,
                arguments: args,
            });
        }
        catch (error) {
            this.log.appendLine(`${method} failed: ${String(error)}`);
            vscode_1.window.showWarningMessage(`Rqx: ${method} could not run — the server is busy or its scanner stopped. See the log.`);
            return undefined;
        }
    }
    /// The rule reported at a line, read back from what the client was actually told
    /// rather than recomputed.
    ruleAt(uri, line) {
        const diagnostic = this.client?.diagnostics
            ?.get(uri)
            ?.find((entry) => entry.source === "rqx" && entry.range.start.line <= line && line <= entry.range.end.line);
        const code = diagnostic?.code;
        if (typeof code === "string") {
            return code;
        }
        return typeof code === "object" && code !== null ? String(code.value) : undefined;
    }
    stop() {
        return this.client?.stop();
    }
}
exports.LanguageServer = LanguageServer;
//# sourceMappingURL=language-server.js.map