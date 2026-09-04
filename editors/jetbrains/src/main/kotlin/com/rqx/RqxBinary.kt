package com.rqx

import com.intellij.openapi.project.Project
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Where to find `rqx`. The plugin ships no binary: one zip carrying five platforms
 * would be fifty megabytes for the one the reader is on, and the release assets are
 * already named by target for the other editors.
 */
object RqxBinary {
    private const val NAME = "rqx"

    fun locate(project: Project): Path {
        RqxSettings.instance.path.takeIf { it.isNotBlank() }?.let { configured ->
            val path = Paths.get(configured)
            if (Files.isExecutable(path)) return path
        }
        onPath()?.let { return it }
        project.basePath?.let { base ->
            val local = Paths.get(base, "target", "release", NAME)
            if (Files.isExecutable(local)) return local
        }
        // Falling back to the bare name lets the IDE's own error say "not found",
        // which is a better message than one invented here about a path nobody set.
        return Paths.get(NAME)
    }

    private fun onPath(): Path? =
        System.getenv("PATH")
            ?.split(java.io.File.pathSeparator)
            ?.asSequence()
            ?.map { Paths.get(it, NAME) }
            ?.firstOrNull { Files.isExecutable(it) }
}
