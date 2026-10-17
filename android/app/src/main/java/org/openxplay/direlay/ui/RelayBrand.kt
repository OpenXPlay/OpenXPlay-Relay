// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import org.openxplay.direlay.R

/** A compact wordmark announced as one product name by accessibility services. */
@Composable fun RelayBrand(modifier: Modifier = Modifier) {
    val name = stringResource(R.string.direlay_app_name)
    Column(modifier.clearAndSetSemantics { contentDescription = name }) {
        Text(stringResource(R.string.relay_brand_first), style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.SemiBold)
        Text(stringResource(R.string.relay_brand_second), style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
