// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

class ReleaseVersion private constructor(val value: String, private val core: List<String>,
    val prerelease: List<String>) : Comparable<ReleaseVersion> {
    override fun compareTo(other: ReleaseVersion): Int {
        core.zip(other.core).forEach { (left, right) -> numericCompare(left, right).let { if (it != 0) return it } }
        if (prerelease.isEmpty() || other.prerelease.isEmpty()) return when {
            prerelease.isEmpty() && other.prerelease.isEmpty() -> 0
            prerelease.isEmpty() -> 1
            else -> -1
        }
        prerelease.zip(other.prerelease).forEach { (left, right) ->
            val leftNumeric = left.all { it in '0'..'9' }
            val rightNumeric = right.all { it in '0'..'9' }
            val order = when {
                leftNumeric && rightNumeric -> numericCompare(left, right)
                leftNumeric -> -1
                rightNumeric -> 1
                else -> left.compareTo(right)
            }
            if (order != 0) return order
        }
        return prerelease.size.compareTo(other.prerelease.size)
    }
    override fun toString() = value

    companion object {
        fun parse(value: String): ReleaseVersion? {
            val buildParts = value.split('+')
            if (buildParts.size > 2 || buildParts.size == 2 && !identifiers(buildParts[1])) return null
            val version = buildParts[0]
            val core = version.substringBefore('-').split('.')
            if (core.size != 3 || core.any { !number(it) }) return null
            val pre = if ('-' in version) version.substringAfter('-').split('.') else emptyList()
            if ('-' in version && (!identifiers(version.substringAfter('-')) ||
                    pre.any { it.all { char -> char in '0'..'9' } && !number(it) })) return null
            return ReleaseVersion(value, core, pre)
        }
        private fun identifiers(value: String) = value.split('.').all { it.isNotEmpty() && it.all(::identifierChar) }
        internal fun identifierChar(char: Char) = char in '0'..'9' || char in 'A'..'Z' || char in 'a'..'z' || char == '-'
        private fun number(value: String) = value.isNotEmpty() && value.all { it in '0'..'9' } && (value.length == 1 || value[0] != '0')
        private fun numericCompare(left: String, right: String) = left.length.compareTo(right.length).takeIf { it != 0 } ?: left.compareTo(right)
    }
}
