// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.update

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

data class UpdateState(val channel: UpdateChannel, val checking: Boolean = false,
    val result: UpdateResult? = null, val failure: UpdateFailure? = null)

class UpdateChecker(private val source: ReleaseSource, private val preferences: UpdatePreferences,
    private val installed: String) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val mutable = MutableStateFlow(UpdateState(preferences.channel))
    val state = mutable.asStateFlow()
    private var releases: List<RelayRelease>? = null

    @Synchronized fun check() {
        if (mutable.value.checking) return
        mutable.value = mutable.value.copy(checking = true, failure = null)
        scope.launch {
            try {
                val fetched = source.fetch()
                synchronized(this@UpdateChecker) {
                    releases = fetched
                    mutable.value = UpdateState(mutable.value.channel, result = selectUpdate(fetched, mutable.value.channel, installed))
                }
            } catch (failure: CancellationException) { throw failure }
            catch (failure: Exception) {
                synchronized(this@UpdateChecker) {
                    mutable.value = UpdateState(mutable.value.channel, failure = failure as? UpdateFailure ?: UpdateFailure.InvalidResponse)
                }
            }
        }
    }

    @Synchronized fun setChannel(channel: UpdateChannel) {
        preferences.channel = channel
        mutable.value = mutable.value.copy(channel = channel,
            result = releases?.let { selectUpdate(it, channel, installed) } ?: mutable.value.result,
            failure = if (releases != null) null else mutable.value.failure)
    }
}
