// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.branding

import org.w3c.dom.Element
import org.w3c.dom.Node
import java.io.StringReader
import java.io.StringWriter
import javax.xml.XMLConstants
import javax.xml.parsers.DocumentBuilderFactory
import javax.xml.transform.OutputKeys
import javax.xml.transform.TransformerFactory
import javax.xml.transform.dom.DOMSource
import javax.xml.transform.stream.StreamResult
import org.xml.sax.InputSource

/** Pure JVM transformation; XML serialization preserves markup and Android escape sequences. */
object BrandingResources {
    const val ATTRIBUTION = "receiver_based_on_xcertplay_licensed_under_gpl_3_0_diplay"
    const val HEADER = "<!-- SPDX-FileCopyrightText: 2026 OpenXPlay.org\n" +
        "     SPDX-License-Identifier: AGPL-3.0-only\n" +
        "     Created by riko chen on 2026-10-14 -->\n"

    private fun parse(xml: String) = DocumentBuilderFactory.newInstance().apply {
        isNamespaceAware = true
        setFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
        setFeature("http://xml.org/sax/features/external-general-entities", false)
        setFeature("http://xml.org/sax/features/external-parameter-entities", false)
    }.newDocumentBuilder().parse(InputSource(StringReader(xml)))

    private fun elements(node: Node): List<Element> = (0 until node.childNodes.length)
        .map { node.childNodes.item(it) }.filterIsInstance<Element>()

    fun keys(xml: String): Set<String> = elements(parse(xml).documentElement)
        .filter { it.tagName in setOf("string", "string-array", "plurals") }
        .map { it.getAttribute("name") }.toSet()

    fun overrides(xml: String, manualKeys: Set<String>): Map<String, String> {
        val root = parse(xml).documentElement
        return elements(root).filter {
            it.tagName in setOf("string", "string-array", "plurals") &&
                it.getAttribute("name") !in manualKeys && it.getAttribute("name") != ATTRIBUTION &&
                it.textContent.contains("DiPlay")
        }.associate { element ->
            // Namespace declarations may live on <resources>, rather than on a styled string.
            for (i in 0 until root.attributes.length) {
                val attribute = root.attributes.item(i)
                if (attribute.namespaceURI == XMLConstants.XMLNS_ATTRIBUTE_NS_URI) {
                    element.setAttributeNS(attribute.namespaceURI, attribute.nodeName, attribute.nodeValue)
                }
            }
            rebrandText(element)
            val writer = StringWriter()
            TransformerFactory.newInstance().newTransformer().apply {
                setOutputProperty(OutputKeys.OMIT_XML_DECLARATION, "yes")
            }.transform(DOMSource(element), StreamResult(writer))
            element.getAttribute("name") to writer.toString()
        }
    }

    private fun rebrandText(node: Node) {
        if (node.nodeType == Node.TEXT_NODE || node.nodeType == Node.CDATA_SECTION_NODE) {
            node.nodeValue = node.nodeValue.replace("DiPlay", "OpenXPlay Relay")
        } else {
            for (i in 0 until node.childNodes.length) rebrandText(node.childNodes.item(i))
        }
    }

    // Replacement introduces no Android metacharacters: retain the original quoting and escapes.
    fun document(entries: Map<String, String>): String = HEADER + "<resources>\n" +
        entries.toSortedMap().values.joinToString("\n") { "    $it" } + "\n</resources>\n"
}
