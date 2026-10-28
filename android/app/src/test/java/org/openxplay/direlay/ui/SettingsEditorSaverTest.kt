// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import android.app.Application
import android.os.Bundle
import android.os.Parcel
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.saveable.SaveableStateRegistry
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.SaverScope
import androidx.compose.runtime.saveable.autoSaver
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.Serializable

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class SettingsEditorSaverTest {
    // Accept only the Bundle-compatible payloads used by these settings savers.
    private fun bundleCompatible(value: Any?): Boolean = when (value) {
        null, is String, is Boolean -> true
        is HashMap<*, *> -> value.all { (key, item) -> key is String && bundleCompatible(item) }
        is List<*> -> value is Serializable && value.all(::bundleCompatible)
        else -> false
    }
    private val scope = object : SaverScope {
        override fun canBeSaved(value: Any) = bundleCompatible(value)
    }

    @Suppress("DEPRECATION", "UNCHECKED_CAST")
    private fun <T : Any, S : Any> roundTrip(saver: Saver<T, S>, value: T): T {
        val encoded = with(saver) { requireNotNull(scope.save(value)) }
        assertTrue("Unsavable payload: ${encoded.javaClass.name}", scope.canBeSaved(encoded))
        // Custom Saver.save does not enforce canBeSaved itself; the registry must validate its result.
        val registry = SaveableStateRegistry(null, scope::canBeSaved)
        registry.registerProvider("editor") { encoded }
        assertEquals(encoded, registry.performSave().getValue("editor").single())
        val bundle = Bundle().apply { putSerializable("editor", encoded as Serializable) }
        val parcel = Parcel.obtain()
        try {
            bundle.writeToParcel(parcel, 0)
            parcel.setDataPosition(0)
            val restored = Bundle.CREATOR.createFromParcel(parcel)
            restored.classLoader = javaClass.classLoader
            return requireNotNull(saver.restore(restored.getSerializable("editor") as S))
        } finally {
            parcel.recycle()
        }
    }

    @Test fun emptyMapsCanBeSavedAtAppLaunch() {
        assertTrue(roundTrip(settingsNumberTextSaver, mutableStateMapOf<String, String>()).isEmpty())
        assertTrue(roundTrip(settingsInvalidNumbersSaver, mutableStateMapOf<String, Boolean>()).isEmpty())
    }

    @Test fun restoredNavigationRepairsOldDuplicatePagesAndKeepsSetupSteps() {
        val legacy = listOf("home", "settings", "home", "setup:car", "settings", "setup:network")
        assertEquals(listOf("home", "setup:network"), roundTrip(relayEntriesSaver, legacy))
        val current = RelayBackStack().open("setup:prepare").open("settings:iphone").entries
        assertEquals(current, roundTrip(relayEntriesSaver, current))
    }

    @Test fun numericEditingTextAndValidationFlagsSurviveBundleRoundTrip() {
        for (entries in listOf(mapOf("width" to ""), mapOf("width" to "", "port" to " 9443", "channel" to "未完成"))) {
            val numbers = mutableStateMapOf<String, String>().apply { putAll(entries) }
            val restored = roundTrip(settingsNumberTextSaver, numbers)
            assertEquals(entries, restored.toMap())
            restored["height"] = "1080"
            assertEquals("1080", restored["height"])
            assertFalse("height" in numbers)
        }
        for (entries in listOf(mapOf("width" to true), mapOf("width" to true, "port" to false))) {
            val errors = mutableStateMapOf<String, Boolean>().apply { putAll(entries) }
            assertEquals(entries, roundTrip(settingsInvalidNumbersSaver, errors).toMap())
        }
    }

    @Test fun savedMapsAreSerializableCopiesRatherThanSnapshotMapViews() {
        val numbers = mutableStateMapOf("width" to "")
        val errors = mutableStateMapOf("width" to true)
        val encodedNumbers = with(settingsNumberTextSaver) { requireNotNull(scope.save(numbers)) }
        val encodedErrors = with(settingsInvalidNumbersSaver) { requireNotNull(scope.save(errors)) }
        numbers["width"] = "1920"
        errors.clear()
        assertEquals(hashMapOf("width" to ""), encodedNumbers)
        assertEquals(hashMapOf("width" to true), encodedErrors)
    }

    @Test fun cardAndCustomDisplayBooleansAndHierarchicalRoutesAreBundleCompatible() {
        for (value in listOf(false, true)) assertEquals(value, roundTrip(autoSaver<Boolean>(), value))
        assertEquals(listOf("home"), roundTrip(autoSaver<List<String>>(), RelayBackStack().entries))
        for (entry in listOf("settings", "settings:sharedWifi", "developer/carlink", "publicName", "vehicle")) {
            val routes = RelayBackStack().open(entry).entries
            assertEquals(routes, roundTrip(autoSaver<List<String>>(), routes))
        }
    }
}
