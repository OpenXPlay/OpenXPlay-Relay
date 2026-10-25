// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.settings

import org.junit.Assert.*
import org.junit.Test

class CarSelectionTest {
    @Test fun completeRequiresBrandAndModelAndNonblankOtherNames() {
        val cases = listOf(
            CarSelection() to false,
            CarSelection("xpeng") to false,
            CarSelection(model = "g6") to false,
            CarSelection("xpeng", model = "g6") to true,
            CarSelection("other", model = "other") to false,
            CarSelection("other", "Brand", "other") to false,
            CarSelection("other", "", "other", "Model") to false,
            CarSelection("other", " \t", "other", "Model") to false,
            CarSelection("other", "Brand", "other", "\n ") to false,
            CarSelection("other", "Brand", "other", "Model") to true,
            CarSelection("other", "Brand", modelText = "Model") to false,
            CarSelection("xpeng", model = "other") to false,
            CarSelection("xpeng", model = "other", modelText = " \t") to false,
            CarSelection("xpeng", model = "other", modelText = " Custom ") to true)
        for ((car, complete) in cases) assertEquals(car.toString(), complete, car.complete)
    }

    @Test fun completenessNeverRequiresAYearOrCustomYearText() {
        for (year in listOf("", "g6-2023", "other")) for (text in listOf("", " ", "Custom year")) {
            assertTrue(CarSelection("xpeng", model = "g6", year = year, yearText = text).complete)
            assertFalse(CarSelection("xpeng", year = year, yearText = text).complete)
        }
        assertTrue(CarSelection("other", "Brand", "other", "Model", "other").complete)
    }

    @Test fun legacyLeavesMigrateWithinTheirBrandOnlyWhenYearIsEmpty() {
        for ((brand, models) in CarSelection.catalogIds) for ((model, years) in models) for (year in years) {
            val expected = CarSelection(brand, model = model, year = year)
            assertEquals(expected, CarSelection(brand, model = year).normalized())
            assertEquals(expected, expected.normalized())
            assertEquals(CarSelection(brand), CarSelection(brand, model = year, year = "other").normalized())
        }
        assertEquals(CarSelection("tesla"), CarSelection("tesla", model = "g6-2023").normalized())
    }

    @Test fun invalidParentsClearDescendantsAndYearsMustBelongToTheSelectedModel() {
        assertEquals(CarSelection(), CarSelection("unknown", "ignored", "g6", "ignored", "g6-2023", "ignored").normalized())
        assertEquals(CarSelection(), CarSelection(model = "g6", year = "g6-2023").normalized())
        assertEquals(CarSelection("xpeng"), CarSelection("xpeng", model = "unknown", year = "g6-2023").normalized())
        assertEquals(CarSelection("xpeng"), CarSelection("xpeng", year = "g6-2023").normalized())
        assertEquals(CarSelection("xpeng", model = "g6"), CarSelection("xpeng", model = "g6", year = "g9-2024").normalized())
        assertEquals(CarSelection("xpeng", model = "g6"), CarSelection("xpeng", model = "g6", year = "unknown").normalized())
    }

    @Test fun otherParentsForceOtherChildrenAndOnlyOtherTextsSurvive() {
        assertEquals(CarSelection("other", "Brand", "other", "Model", "other", "Year"),
            CarSelection("other", " Brand\n", "g6", " Model\t", "g6-2023", " Year\r").normalized())
        assertEquals(CarSelection("xpeng", model = "other", modelText = "Model", year = "other", yearText = "Year"),
            CarSelection("xpeng", "ignored", "other", " Model ", "g6-2023", " Year ").normalized())
        assertEquals(CarSelection("xpeng", model = "g6", year = "other", yearText = "Year"),
            CarSelection("xpeng", "ignored", "g6", "ignored", "other", " Year ").normalized())
        assertEquals(CarSelection("xpeng", model = "g6", year = "g6-2023"),
            CarSelection("xpeng", "ignored", "g6", "ignored", "g6-2023", "ignored").normalized())
        val bounded = CarSelection("other", "b".repeat(60), "other", "m".repeat(60), "other", "y".repeat(60)).normalized()
        assertEquals(40, bounded.brandText.length); assertEquals(40, bounded.modelText.length); assertEquals(40, bounded.yearText.length)
        assertEquals(bounded, bounded.normalized())
    }
}
