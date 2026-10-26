// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "OpenXPlay-Relay"
include(":app")

val diplayDir = providers.gradleProperty("diplay.dir").orNull
    ?: if (file("../third_party/OpenXPlay-DiPlay").isDirectory) {
        "../third_party/OpenXPlay-DiPlay"
    } else {
        "../../OpenXPlay-DiPlay"
    }

includeBuild(diplayDir) {
    dependencySubstitution {
        substitute(module("org.openxplay.diplay:common")).using(project(":common"))
        substitute(module("org.openxplay.diplay:shared")).using(project(":shared"))
    }
}

val streamDir = providers.gradleProperty("stream.dir").orNull
    ?: if (file("../third_party/OpenXPlay-Stream/kotlin").isDirectory) {
        "../third_party/OpenXPlay-Stream/kotlin"
    } else {
        "../../OpenXPlay-Stream/kotlin"
    }

// Releases build the OpenXPlay-Stream and OpenXPlay-DiPlay commits in stream.lock and diplay.lock; say when a
// checkout builds something else. -Prelay.strictSources=true (CI) stops the build instead.
val strictSources = providers.gradleProperty("relay.strictSources").orNull == "true"
fun checkSource(name: String, lock: String, checkout: File) {
    fun git(vararg args: String): String? = runCatching {
        providers.exec { commandLine("git", "-C", checkout.path, *args); isIgnoreExitValue = true }.standardOutput.asText.get().trim()
    }.getOrNull()?.takeIf { it.isNotEmpty() }
    val locked = file("../$lock").takeIf { it.isFile }?.readText()?.trim().orEmpty()
    // A copy inside another repository (a source archive under third_party/) is not a checkout of its own.
    val head = git("rev-parse", "--show-toplevel")?.takeIf { file(it).canonicalFile == checkout }?.let { git("rev-parse", "HEAD") }
    val problems = listOfNotNull(
        "$lock is missing.".takeIf { strictSources && locked.isEmpty() },
        "$name at ${checkout.path} is not a git checkout.".takeIf { strictSources && head == null },
        "$name at ${checkout.path} is ${head?.take(12)}, not ${locked.take(12)} from $lock."
            .takeIf { head != null && locked.isNotEmpty() && head != locked },
        "$name at ${checkout.path} has uncommitted changes.".takeIf { head != null && git("status", "--porcelain") != null },
    )
    if (strictSources && problems.isNotEmpty()) throw GradleException(problems.joinToString(" "))
    problems.forEach { logger.warn(it) }
}
checkSource("OpenXPlay-DiPlay", "diplay.lock", file(diplayDir).canonicalFile)
checkSource("OpenXPlay-Stream", "stream.lock", file(streamDir).resolve("..").canonicalFile)

includeBuild(streamDir) {
    dependencySubstitution {
        substitute(module("org.openxplay.stream:core")).using(project(":core"))
        substitute(module("org.openxplay.stream:wire")).using(project(":wire"))
        substitute(module("org.openxplay.stream:web-server")).using(project(":web-server"))
        // Carlink output uses the same local Stream composite as core and web-server.
        substitute(module("org.openxplay.stream:clink")).using(project(":clink"))
        substitute(module("org.openxplay.stream:cast")).using(project(":cast"))
    }
}
