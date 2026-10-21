// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.publicname

import org.openxplay.direlay.tls.TlsStore
import java.security.PrivateKey
import java.security.cert.X509Certificate

/** The TLS store's ACME source as the public name service sees it. */
class TlsStoreCertificates(private val tls: TlsStore) : AcmeCertificates {
    override fun acmeCertificate(): AcmeCertificateInfo? = tls.acmeMaterial()?.let { material ->
        AcmeCertificateInfo(material.name, material.leaf.notBefore.time, material.leaf.notAfter.time,
            runCatching { AcmeCrypto.certId(material.leaf) }.getOrNull())
    }
    override fun installAcme(name: String, chain: List<X509Certificate>, key: PrivateKey) = tls.installAcme(name, chain, key)
    override fun removeAcme() = tls.removeAcme()
    override fun serveAcme(name: String?) = tls.serveAcme(name)
}
