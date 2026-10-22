// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.publicname.DnsProviderId
import org.openxplay.direlay.publicname.PublicNameProviderView
import org.openxplay.direlay.publicname.PublicNameView

class PublicNameDraftsTest {
    private val duck = DnsProviderId.DUCKDNS
    private val dyn = DnsProviderId.DYNV6
    private val view = PublicNameView(configured = true, provider = duck, providers = mapOf(
        duck to PublicNameProviderView("saved.duckdns.org", true),
        dyn to PublicNameProviderView("saved.dynv6.net", false),
    ))

    @Test fun switchingShowsEachProvidersSavedNameAndTokenFlagWithAnEmptyTokenField() {
        val drafts = PublicNameDrafts()
        assertEquals(PublicNameDraft("saved.duckdns.org", tokenSet = true), drafts.fields(duck, view))
        assertEquals(PublicNameDraft("saved.dynv6.net", tokenSet = false), drafts.fields(dyn, view))
        assertEquals(PublicNameDraft("saved.duckdns.org", tokenSet = true), drafts.fields(duck, view))
        assertEquals(PublicNameDraft(""), drafts.fields(dyn, PublicNameView()))
    }

    @Test fun unsavedNamesAndTypedTokensSurviveSwitchingInBothDirections() {
        var drafts = PublicNameDrafts()
        drafts = drafts.edit(duck, drafts.fields(duck, view).copy(name = "draft.duckdns.org", token = "duck-secret"))
        assertEquals(PublicNameDraft("saved.dynv6.net"), drafts.fields(dyn, view))
        drafts = drafts.edit(dyn, drafts.fields(dyn, view).copy(name = "draft.dynv6.net", token = "dyn-secret"))
        assertEquals(PublicNameDraft("draft.duckdns.org", "duck-secret", true), drafts.fields(duck, view))
        assertEquals(PublicNameDraft("draft.dynv6.net", "dyn-secret", false), drafts.fields(dyn, view))
        assertFalse(drafts.fields(duck, view).toString().contains("duck-secret"))
        assertFalse(drafts.fields(dyn, view).toString().contains("dyn-secret"))
    }

    @Test fun savingOneProviderRefreshesItsFieldsAndKeepsTheOtherProvidersDraft() {
        val duckDraft = PublicNameDraft(" New.DuckDNS.org ", "duck-secret", true)
        val dynDraft = PublicNameDraft("draft.dynv6.net", "dyn-secret")
        val drafts = PublicNameDrafts().edit(duck, duckDraft).edit(dyn, dynDraft).saved(duck, duckDraft)
        val savedView = view.copy(revision = 1, providers = view.providers + (duck to PublicNameProviderView("new.duckdns.org", true)))
        assertEquals(PublicNameDraft("new.duckdns.org", tokenSet = true), drafts.fields(duck, savedView))
        assertEquals(dynDraft, drafts.fields(dyn, savedView))
        val importedView = savedView.copy(providers = savedView.providers + (dyn to PublicNameProviderView("imported.dynv6.net", true)))
        assertEquals(dynDraft.copy(tokenSet = true), drafts.fields(dyn, importedView))
    }

    @Test fun completedSaveKeepsEditsMadeWhileItWasRunningAndClearsOnlyTheSubmittedToken() {
        val submitted = PublicNameDraft("submitted.dynv6.net", "submitted-secret")
        val renamed = submitted.copy(name = "newer.dynv6.net")
        val withNewName = PublicNameDrafts().edit(dyn, renamed).saved(dyn, submitted)
        assertEquals(renamed.copy(token = ""), withNewName.fields(dyn, view))
        val newToken = renamed.copy(token = "newer-secret")
        val withNewToken = PublicNameDrafts().edit(dyn, newToken).saved(dyn, submitted)
        assertEquals(newToken, withNewToken.fields(dyn, view))
    }
}
