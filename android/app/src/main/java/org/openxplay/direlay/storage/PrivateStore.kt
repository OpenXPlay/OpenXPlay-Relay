// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.storage

import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.StandardCopyOption.ATOMIC_MOVE
import java.nio.file.StandardCopyOption.REPLACE_EXISTING
import java.nio.file.attribute.PosixFilePermissions

sealed class MaterialStatus {
    data object Installed : MaterialStatus()
    data object Missing : MaterialStatus()
    data class Invalid(val reason: String) : MaterialStatus()
}

/** A validated generation is published by one atomic rename of a directory symlink. */
internal class PrivateStore(private val root: File, private val name: String) {
    val directory = File(root, name)
    private val prefix = ".$name-generation-"

    fun replace(writeAndValidate: (File) -> Unit) {
        // Adopt an unmanaged directory as an old generation so it is cleaned up after the switch.
        if (directory.exists() && !Files.isSymbolicLink(directory.toPath())) {
            Files.move(directory.toPath(), File(root, "$prefix${java.util.UUID.randomUUID()}").toPath(), ATOMIC_MOVE)
        }
        val staging = Files.createTempDirectory(root.toPath(), prefix,
            PosixFilePermissions.asFileAttribute(PosixFilePermissions.fromString("rwx------"))).toFile()
        val link = File(root, ".$name-link-${java.util.UUID.randomUUID()}")
        var installed = false
        try {
            writeAndValidate(staging)
            Files.createSymbolicLink(link.toPath(), staging.toPath().fileName)
            Files.move(link.toPath(), directory.toPath(), ATOMIC_MOVE, REPLACE_EXISTING)
            installed = true
        } finally {
            Files.deleteIfExists(link.toPath())
            if (!installed) staging.deleteRecursively()
        }
        // Key material must not outlive its replacement: delete every other generation.
        root.listFiles().orEmpty()
            .filter { it.name.startsWith(prefix) && it.name != staging.name }
            .forEach { it.deleteRecursively() }
    }

    fun remove() {
        if (Files.isSymbolicLink(directory.toPath())) Files.delete(directory.toPath())
        else check(!directory.exists() || directory.deleteRecursively()) { "Could not remove installed files" }
        root.listFiles().orEmpty().filter { it.name.startsWith(prefix) }.forEach {
            check(it.deleteRecursively()) { "Could not remove private generation" }
        }
    }

    fun status(validate: (File) -> Unit): MaterialStatus {
        if (!Files.exists(directory.toPath(), NOFOLLOW_LINKS)) return MaterialStatus.Missing
        return try {
            validate(directory.canonicalFile)
            MaterialStatus.Installed
        } catch (_: Exception) {
            MaterialStatus.Invalid("Stored files are incomplete or failed validation")
        }
    }

    companion object {
        fun write(directory: File, name: String, bytes: ByteArray) {
            val path = File(directory, name).toPath()
            Files.createFile(path, PosixFilePermissions.asFileAttribute(PosixFilePermissions.fromString("rw-------")))
            java.io.FileOutputStream(path.toFile()).use { it.write(bytes); it.fd.sync() }
        }
    }
}
