// The plugin is a thin client: it starts `rqx lsp` and lets LSP4IJ do the protocol.
// Everything a reader sees — diagnostics, hovers, quick fixes, the catalogue — comes
// from the server, so a capability added there appears here without a release.
plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "2.0.21"
    id("org.jetbrains.intellij.platform") version "2.1.0"
}

group = "com.rqx"
version = providers.gradleProperty("pluginVersion").getOrElse("0.1.0")

repositories {
    mavenCentral()
    intellijPlatform { defaultRepositories() }
}

dependencies {
    intellijPlatform {
        // The lowest IDE that carries the APIs LSP4IJ needs. Community editions are
        // the point: the built-in LSP API is Ultimate-only, which would leave out
        // PyCharm CE and IDEA CE — most of the people who would install this.
        intellijIdeaCommunity("2024.2")
        plugin("com.redhat.devtools.lsp4ij:0.7.0")
        instrumentationTools()
    }
}

kotlin { jvmToolchain(17) }

intellijPlatform {
    pluginConfiguration {
        ideaVersion {
            sinceBuild = "242"
            untilBuild = provider { null }
        }
    }
    publishing { token = providers.environmentVariable("JETBRAINS_MARKETPLACE_TOKEN") }
}
