// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.branding

import org.junit.Assert.*
import org.junit.Test

class BrandingResourcesTest {
    @Test fun replacesStringsAndWholeCollectionsWhileKeepingOtherItems() {
        val xml = """<resources>
            <string name="app_name">DiPlay</string>
            <string name="manual">DiPlay custom</string>
            <string name="unrelated">Hello</string>
            <string name="${BrandingResources.ATTRIBUTION}">DiPlay and xcertplay</string>
            <string-array name="choices"><item>DiPlay</item><item>Other</item></string-array>
            <plurals name="count"><item quantity="one">DiPlay %d</item><item quantity="other">%d apps</item></plurals>
        </resources>"""
        val result = BrandingResources.overrides(xml, setOf("manual"))
        assertEquals(setOf("app_name", "choices", "count"), result.keys)
        assertTrue(result.getValue("choices").contains("<item>Other</item>"))
        assertTrue(result.getValue("count").contains("quantity=\"other\""))
        assertFalse(result.values.any { it.contains("DiPlay") })
    }

    @Test fun preservesAndroidEscapesQuotedStringsCdataAndStyledMarkup() {
        val xml = """<resources xmlns:xliff="urn:oasis:names:tc:xliff:document:1.2">
            <string name="escaped">DiPlay\'s \"name\"\n\u0020 \\ &amp; &lt;ok&gt;</string>
            <string name="quoted">"DiPlay's name"</string>
            <string name="cdata"><![CDATA[DiPlay\'s <literal> & \n]]></string>
            <string name="styled"><b>DiPlay</b> <xliff:g id="name">%1${'$'}s</xliff:g></string>
        </resources>"""
        val result = BrandingResources.overrides(xml, emptySet())
        assertTrue(result.getValue("escaped").contains("OpenXPlay Relay\\'s \\\"name\\\"\\n\\u0020 \\\\ &amp; &lt;ok&gt;"))
        assertTrue(result.getValue("quoted").contains("\"OpenXPlay Relay's name\""))
        assertTrue(result.getValue("cdata").contains("<![CDATA[OpenXPlay Relay\\'s <literal> & \\n]]>"))
        assertTrue(result.getValue("styled").contains("<b>OpenXPlay Relay</b>"))
        assertTrue(result.getValue("styled").contains("xmlns:xliff="))
        val document = BrandingResources.document(result)
        assertEquals(result.keys, BrandingResources.keys(document))
        assertTrue(BrandingResources.overrides(document, emptySet()).isEmpty())
    }

    @Test fun replacementDoesNotChangeResourceAttributesOrReferences() {
        val result = BrandingResources.overrides("""<resources>
            <string name="label" translatable="false" formatted="false">DiPlay</string>
            <string name="reference">@string/diplay</string>
        </resources>""", emptySet())
        assertEquals(setOf("label"), result.keys)
        assertTrue(result.getValue("label").contains("formatted=\"false\""))
        assertTrue(result.getValue("label").contains("translatable=\"false\""))
    }
}
