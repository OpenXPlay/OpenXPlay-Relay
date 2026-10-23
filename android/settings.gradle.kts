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

// Releases build the OpenXPlay-Stream commit in stream.lock; say when this checkout builds something else.
run {
    val pinned = file("../stream.lock").takeIf { it.isFile }?.readText()?.trim().orEmpty()
    val root = file(streamDir).resolve("..").canonicalFile
    fun git(vararg args: String): String? = runCatching {
        providers.exec { commandLine("git", "-C", root.path, *args); isIgnoreExitValue = true }.standardOutput.asText.get().trim()
    }.getOrNull()
    val head = git("rev-parse", "HEAD")
    if (pinned.isNotEmpty() && !head.isNullOrEmpty() && head != pinned)
        logger.warn("OpenXPlay-Stream at ${root.path} is ${head.take(12)}, not ${pinned.take(12)} from stream.lock.")
    if (!git("status", "--porcelain").isNullOrEmpty()) logger.warn("OpenXPlay-Stream at ${root.path} has uncommitted changes.")
}

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
