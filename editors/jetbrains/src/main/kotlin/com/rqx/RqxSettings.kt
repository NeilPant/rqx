package com.rqx

import com.intellij.openapi.components.*

@Service(Service.Level.APP)
@State(name = "RqxSettings", storages = [Storage("rqx.xml")])
class RqxSettings : SimplePersistentStateComponent<RqxSettings.State>(State()) {
    class State : BaseState() {
        var path by string("")
        var scanOnStartup by property(true)
        var scanOnSave by property(true)
        var codeLens by property(false)
        var patterns by property(false)
        var base by string("origin/HEAD")
    }

    val path: String get() = state.path.orEmpty()
    val scanOnStartup: Boolean get() = state.scanOnStartup
    val scanOnSave: Boolean get() = state.scanOnSave
    val codeLens: Boolean get() = state.codeLens
    val patterns: Boolean get() = state.patterns
    val base: String get() = state.base ?: "origin/HEAD"

    companion object {
        val instance: RqxSettings get() = service()
    }
}
