// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.settings

data class CarSelection(val brand: String = "", val brandText: String = "", val model: String = "", val modelText: String = "",
    val year: String = "", val yearText: String = "") {
    val complete get() = brand.isNotEmpty() && model.isNotEmpty() &&
        (brand != "other" || brandText.isNotBlank()) && (model != "other" || modelText.isNotBlank())

    fun normalized(): CarSelection {
        val brand = this.brand.takeIf { it.isEmpty() || it == "other" || it in catalogIds }.orEmpty()
        val models = catalogIds[brand].orEmpty()
        val legacyModel = models.entries.firstOrNull { this.year.isEmpty() && this.model in it.value }?.key
        val model = when (brand) {
            "other" -> "other"
            "" -> ""
            else -> legacyModel ?: this.model.takeIf { it.isEmpty() || it == "other" || it in models }.orEmpty()
        }
        val year = when (model) {
            "other" -> "other"
            "" -> ""
            else -> if (legacyModel != null) this.model
                else this.year.takeIf { it.isEmpty() || it == "other" || it in models.getValue(model) }.orEmpty()
        }
        return CarSelection(brand, if (brand == "other") cleanText(brandText).trim() else "", model,
            if (model == "other") cleanText(modelText).trim() else "", year,
            if (year == "other") cleanText(yearText).trim() else "")
    }
    companion object {
        // Normalization also runs without an Android asset context; CarModelsTest checks asset parity.
        internal val catalogIds = mapOf(
            "xpeng" to mapOf(
                "g3" to setOf("g3-2019", "g3-2020", "g3-2021", "g3-2022"),
                "p7" to setOf("p7-2020", "p7-2021", "p7-2022", "p7-2023", "p7-2024", "p7-2025"),
                "p5" to setOf("p5-2021", "p5-2022", "p5-2023", "p5-2024"),
                "g9" to setOf("g9-2022", "g9-2024", "g9-2025", "g9-2026"),
                "g6" to setOf("g6-2023", "g6-2024", "g6-2025", "g6-2026"),
                "x9" to setOf("x9-2024", "x9-2025", "x9-2026"),
                "mona-m03" to setOf("mona-m03-2024", "mona-m03-2025", "mona-m03-2026"),
                "p7-plus" to setOf("p7-plus-2024", "p7-plus-2025", "p7-plus-2026"),
                "g7" to setOf("g7-2025", "g7-2026"),
                "gx" to setOf("gx-2026"),
                "mona-l03" to setOf("mona-l03-2026"),
                "g9l" to setOf("g9l-2026")),
            "tesla" to mapOf(
                "model-3" to setOf("model-3-2017", "model-3-2024"),
                "model-y" to setOf("model-y-2020", "model-y-2025"),
                "model-y-l" to setOf("model-y-l-2025"),
                "model-s" to setOf("model-s-2012", "model-s-2021"),
                "model-x" to setOf("model-x-2015", "model-x-2021"),
                "cybertruck" to setOf("cybertruck-2023"))
        )
        fun cleanText(value: String) = value.filterNot { Character.isISOControl(it) }.take(40)
    }
}
