// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.content.pm.ActivityInfo
import com.shilapi.xcertplay.CarPlayHostActivity
import com.shilapi.xcertplay.DiPlayActivity
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.path.StepAction
import org.openxplay.direlay.ui.RelayBackStack
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class RelayActivityIntentTest {
    private val context = RuntimeEnvironment.getApplication()

    @Test fun everyFactoryIntentHasTheSameNonDestructiveTaskFlags() {
        for (intent in listOf(RelayActivity.intent(context), RelayActivity.intent(context, "home"),
                RelayActivity.intent(context, "setup:network", StepAction.CHOOSE_RENDERER))) {
            assertEquals(ComponentName(context, RelayActivity::class.java), intent.component)
            assertEquals(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or Intent.FLAG_ACTIVITY_SINGLE_TOP, intent.flags)
            assertEquals(0, intent.flags and (Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_CLEAR_TASK or
                Intent.FLAG_ACTIVITY_MULTIPLE_TASK or Intent.FLAG_ACTIVITY_NEW_DOCUMENT))
        }
        assertNull(RelayActivity.intent(context).extras)
        val intent = RelayActivity.intent(context, "vehicle", StepAction.CHOOSE_RENDERER)
        assertEquals("vehicle", intent.getStringExtra(RelayActivity.EXTRA_HOME_DESTINATION))
        assertEquals(StepAction.CHOOSE_RENDERER.name, intent.getStringExtra(RelayActivity.EXTRA_STEP_ACTION))
    }

    @Suppress("DEPRECATION")
    @Test fun mergedManifestIsolatesTheRelaySingletonFromTheCarPlayTask() {
        val manager = context.packageManager
        val relay = manager.getActivityInfo(ComponentName(context, RelayActivity::class.java), 0)
        val host = manager.getActivityInfo(ComponentName(context, CarPlayHostActivity::class.java), 0)
        assertEquals(ActivityInfo.LAUNCH_SINGLE_TASK, relay.launchMode)
        assertEquals(context.packageName + ".relay", relay.taskAffinity)
        assertEquals(ActivityInfo.LAUNCH_SINGLE_TASK, host.launchMode)
        assertEquals(context.packageName + ".carplay", host.taskAffinity)
        assertNotEquals(relay.taskAffinity, host.taskAffinity)
        assertFalse(manager.getActivityInfo(ComponentName(context, DiPlayActivity::class.java), 0).exported)
        assertEquals(ComponentName(context, RelayActivity::class.java), manager.getLaunchIntentForPackage(context.packageName)?.component)
    }

    @Test fun destinationsAndActionsAreConsumedOnceWithoutARuntime() {
        for (destination in listOf("home", "setup:network", "settings:", "settings:car", "settings:iphone",
                "developer/video", "experimental", "vehicle", "snapshot:12", "publicName", "about", "settings/tesla")) {
            val intent = RelayActivity.intent(context, destination, StepAction.CHOOSE_RENDERER)
            val request = RelayLaunchRequest.take(intent)
            assertNotNull(destination, request.destination)
            assertEquals(StepAction.CHOOSE_RENDERER, request.stepAction)
            val stack = RelayBackStack().open("vehicle").open(requireNotNull(request.destination))
            assertEquals(stack, stack.open(requireNotNull(request.destination)))
            assertFalse(intent.hasExtra(RelayActivity.EXTRA_HOME_DESTINATION))
            assertFalse(intent.hasExtra(RelayActivity.EXTRA_STEP_ACTION))
            assertEquals(RelayLaunchRequest(null, null), RelayLaunchRequest.take(intent))
        }
        val malformed = Intent().putExtra(RelayActivity.EXTRA_HOME_DESTINATION, "not-a-page")
            .putExtra(RelayActivity.EXTRA_STEP_ACTION, "not-an-action")
        assertEquals(RelayLaunchRequest(null, null), RelayLaunchRequest.take(malformed))
    }

    @Test fun repeatedNewIntentDestinationsNeverAppendHomeOrAnExistingPage() {
        var stack = RelayBackStack().open("setup").open("settings:network").open("vehicle")
        repeat(10) {
            val request = RelayLaunchRequest.take(RelayActivity.intent(context, "home"))
            stack = stack.open(requireNotNull(request.destination))
            assertEquals(listOf("home"), stack.entries)
        }
    }

    @Test fun theDiPlayLauncherShimStillReturnsHomeOrOpensItsRequestedSettingsPage() {
        for ((legacy, expected) in mapOf("home" to "home", "settings" to "settings",
                "wireless-recovery" to "settings/connection/network")) {
            val intent = Intent().putExtra("page", legacy)
            assertEquals(expected, RelayLaunchRequest.take(intent).destination)
            assertEquals(RelayLaunchRequest(null, null), RelayLaunchRequest.take(intent))
        }
        val explicit = RelayActivity.intent(context, "vehicle").putExtra("page", "home")
        assertEquals("vehicle", RelayLaunchRequest.take(explicit).destination)
    }

    @Test fun processRestorationDiscardsOriginalLaunchExtrasInsteadOfReplayingThem() {
        val original = RelayActivity.intent(context, "setup:car", StepAction.GRANT_PERMISSIONS)
        val restored = Intent(original)
        assertEquals(RelayLaunchRequest("setup:car", StepAction.GRANT_PERMISSIONS), RelayLaunchRequest.take(original))
        assertEquals(RelayLaunchRequest(null, null), RelayLaunchRequest.take(restored, restoring = true))
        assertFalse(restored.hasExtra(RelayActivity.EXTRA_HOME_DESTINATION))
        assertFalse(restored.hasExtra(RelayActivity.EXTRA_STEP_ACTION))
    }

    @Test fun allInternalEntryPointsUseTheFactoryAndPairingKeepsTheDefaultDisplay() {
        val root = File("src/main/java/org/openxplay/direlay")
        for (path in listOf("RelayBootReceiver.kt", "service/RelayKeepAliveService.kt", "admin/AndroidBluetoothVisibility.kt",
                "relay/RelaySessionActions.kt", "DiRelayApplication.kt", "presentation/CarPlayHostOverlay.kt")) {
            val source = root.resolve(path).readText()
            assertTrue(path, source.contains("RelayActivity.intent("))
            assertFalse(path, source.contains("RelayActivity::class.java"))
        }
        assertTrue(root.resolve("DiRelayApplication.kt").readText().contains("setLaunchDisplayId(Display.DEFAULT_DISPLAY)"))
        val activity = root.resolve("RelayActivity.kt").readText()
        assertTrue(activity.substringAfter("override fun onNewIntent").substringBefore("private fun startCarPlay").contains("handleStepIntent(intent)"))
        assertTrue(activity.substringBefore("private fun handleImportIntent").contains("handleStepIntent(intent, restoring = savedInstanceState != null)"))
    }
}
