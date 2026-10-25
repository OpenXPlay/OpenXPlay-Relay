// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.settings

import org.json.JSONArray
import org.json.JSONObject

typealias PictureSize = WirelessPictureSize

/** Null trial means the default; a trial with null size explicitly chooses Automatic. */
data class CarlinkPictureTrial(val size: PictureSize?)

val PictureSize.ratioLabel: String? get() {
    var a = width
    var b = height
    while (b != 0) { val rest = a % b; a = b; b = rest }
    if (a <= 0) return null
    val w = width / a
    val h = height / a
    // Screens call 8:5 by its usual name.
    return when { w == 8 && h == 5 -> "16:10"; w <= 32 && h <= 32 -> "$w:$h"; else -> null }
}

object CarlinkPictureSizes {
    const val MAX_SAVED = 5
    val presets = listOf(PictureSize(1280, 720), PictureSize(1920, 1080), PictureSize(1280, 800),
        PictureSize(1920, 720), PictureSize(1024, 600), PictureSize(1024, 768))
    fun parse(width: String, height: String): PictureSize? = width.toIntOrNull()?.let { w -> height.toIntOrNull()?.let { h ->
        PictureSize(w, h).takeIf { runCatching { it.validate() }.isSuccess }
    } }
    fun normalized(sizes: List<PictureSize>) = sizes.filter {
        it !in presets && runCatching { it.validate() }.isSuccess
    }.distinct().take(MAX_SAVED)
    fun read(value: Any?): PictureSize? {
        val json = value as? JSONObject ?: return null
        val width = json.opt("width") as? Int ?: return null
        val height = json.opt("height") as? Int ?: return null
        return parse(width.toString(), height.toString())
    }
    fun json(size: PictureSize) = JSONObject().put("width", size.width).put("height", size.height)
    fun readSaved(array: JSONArray) = normalized((0 until array.length()).mapNotNull { read(array.opt(it)) })
    fun savedJson(sizes: List<PictureSize>) = JSONArray().also { array -> sizes.forEach { array.put(json(it)) } }
    fun decodeSaved(text: String) = runCatching { readSaved(JSONArray(text)) }.getOrDefault(emptyList())
    fun decodeDefault(text: String?) = text?.let { runCatching { read(JSONObject(it)) }.getOrNull() }
}

fun RelaySettings.normalizedCarlinkPictures(): RelaySettings {
    val sizes = CarlinkPictureSizes.normalized(carlinkPictureSizes)
    return copy(carlinkPictureSizes = sizes, carlinkPictureDefault = carlinkPictureDefault?.takeIf {
        it in CarlinkPictureSizes.presets || it in sizes
    })
}
fun RelaySettings.effectiveCarlinkPicture(trial: CarlinkPictureTrial?): PictureSize? =
    if (trial != null) trial.size else carlinkPictureDefault

fun RelaySettings.deleteCarlinkPicture(size: PictureSize) = copy(carlinkPictureSizes = carlinkPictureSizes - size,
    carlinkPictureDefault = carlinkPictureDefault.takeUnless { it == size })
