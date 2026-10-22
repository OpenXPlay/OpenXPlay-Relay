// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import org.openxplay.direlay.publicname.DnsProviderId
import org.openxplay.direlay.publicname.PublicNameView

/** Typed tokens are memory-only and redacted even from a draft's diagnostic string. */
internal data class PublicNameDraft(val name: String, val token: String = "", val tokenSet: Boolean = false) {
    override fun toString() = "PublicNameDraft(name=$name, token=${if (token.isEmpty()) "none" else "set"}, tokenSet=$tokenSet)"
}

/** Overrides only edited providers; untouched fields follow the latest saved view. Never save to a Bundle. */
internal class PublicNameDrafts(private val edits: Map<DnsProviderId, PublicNameDraft> = emptyMap()) {
    fun fields(provider: DnsProviderId, view: PublicNameView): PublicNameDraft {
        val saved = view.providers[provider]
        return (edits[provider] ?: PublicNameDraft(saved?.name.orEmpty())).copy(tokenSet = saved?.tokenSet == true)
    }

    fun edit(provider: DnsProviderId, fields: PublicNameDraft) = PublicNameDrafts(edits + (provider to fields))

    /** A completed save clears only the submitted token, preserving other providers and newer edits. */
    fun saved(provider: DnsProviderId, submitted: PublicNameDraft): PublicNameDrafts {
        val current = edits[provider] ?: return this
        if (current.name == submitted.name && current.token == submitted.token) return PublicNameDrafts(edits - provider)
        return edit(provider, current.copy(token = if (current.token == submitted.token) "" else current.token))
    }
}
