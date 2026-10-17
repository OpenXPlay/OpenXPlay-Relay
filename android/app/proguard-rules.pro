# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-15

# R8 optimizes Compose, AndroidX, Kotlin and OpenXPlay Relay's own code. Code that is reflective, native or
# protocol-heavy stays exactly as built, so CarPlay, crypto, Bonjour and the receivers behave like
# the debug build.
-keep class com.shilapi.** { *; }
-keep class org.concentus.** { *; }
-keep class org.bouncycastle.** { *; }
-keep class javax.jmdns.** { *; }
-keep class org.openxplay.stream.** { *; }
-keep class com.google.zxing.** { *; }
-keepattributes *Annotation*, Signature, InnerClasses, EnclosingMethod, SourceFile, LineNumberTable

# BouncyCastle's LDAP certificate store references JNDI, which Android does not have and OpenXPlay Relay
# never uses.
-dontwarn javax.naming.**
