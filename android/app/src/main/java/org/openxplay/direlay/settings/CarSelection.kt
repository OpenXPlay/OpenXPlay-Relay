// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.settings

data class CarSelection(val brand: String = "", val brandText: String = "", val model: String = "", val modelText: String = "") {
    fun normalized(): CarSelection {
        val brand = this.brand.takeIf { it.isEmpty() || it == "other" || it in models }.orEmpty()
        val model = when (brand) {
            "other" -> "other"
            "" -> ""
            else -> this.model.takeIf { it.isEmpty() || it == "other" || it in models.getValue(brand) }.orEmpty()
        }
        return CarSelection(brand, if (brand == "other") cleanText(brandText).trim() else "", model,
            if (model == "other") cleanText(modelText).trim() else "")
    }
    companion object {
        private val models = mapOf("xpeng" to setOf("g6-2023"), "tesla" to setOf(
            "model-3-2017", "model-3-2024", "model-y-2020", "model-y-2025", "model-y-l-2025",
            "model-s-2021", "model-s-2012", "model-x-2021", "model-x-2015", "cybertruck-2023"))
        fun cleanText(value: String) = value.filterNot { Character.isISOControl(it) }.take(40)
    }
}
