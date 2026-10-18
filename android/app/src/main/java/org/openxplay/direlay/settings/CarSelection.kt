// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.settings

data class CarSelection(val brand: String = "", val brandText: String = "", val model: String = "", val modelText: String = "") {
    fun normalized(): CarSelection {
        val brand = this.brand.takeIf { it in listOf("", "xpeng", "other") }.orEmpty()
        val model = if (brand == "other") "other" else if (brand == "xpeng")
            this.model.takeIf { it in listOf("", "g6-2023", "other") }.orEmpty() else ""
        return CarSelection(brand, if (brand == "other") cleanText(brandText).trim() else "", model,
            if (model == "other") cleanText(modelText).trim() else "")
    }
    companion object {
        fun cleanText(value: String) = value.filterNot { Character.isISOControl(it) }.take(40)
    }
}
