// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import org.openxplay.direlay.R
import org.openxplay.direlay.publicname.ConfigError
import org.openxplay.direlay.publicname.ConfigProblem
import org.openxplay.direlay.publicname.DnsProviderId
import org.openxplay.direlay.publicname.PublicNameEdit
import org.openxplay.direlay.publicname.PublicNameError
import org.openxplay.direlay.publicname.PublicNameFile
import org.openxplay.direlay.publicname.PublicNameState
import org.openxplay.direlay.publicname.PublicNameStatus
import org.openxplay.direlay.publicname.PublicNameView
import java.text.DateFormat
import java.util.Date

/**
 * Settings → Trusted HTTPS. The token typed here lives only in this composition: it is never put into a
 * saved-state Bundle and never shown back; an empty field keeps the saved token.
 */
@Composable fun PublicNameScreen(view: PublicNameView, status: PublicNameStatus, url: String?, back: () -> Unit,
    save: suspend (PublicNameEdit) -> List<ConfigError>?, importFile: () -> Unit, renew: () -> Unit, remove: suspend () -> Boolean) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Keyed by the revision: a save, an import or a removal elsewhere resets the fields to what is stored.
    var enabled by rememberSaveable(view.revision) { mutableStateOf(view.enabled) }
    var provider by rememberSaveable(view.revision) { mutableStateOf((view.provider ?: DnsProviderId.DUCKDNS).key) }
    var name by rememberSaveable(view.revision) { mutableStateOf(view.name) }
    var terms by rememberSaveable(view.revision) { mutableStateOf(view.agreeTerms) }
    var token by remember(view.revision) { mutableStateOf("") }
    // Not keyed: the save that sets them also changes the revision.
    var errors by remember { mutableStateOf(emptyMap<String, Int>()) }
    var saved by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var confirmRemove by remember { mutableStateOf(false) }
    val providers = DnsProviderId.entries
    DetailPage(stringResource(R.string.public_name_title), back) {
        Hint(stringResource(R.string.public_name_summary))
        Toggle(stringResource(R.string.public_name_enabled), enabled) { enabled = it; saved = false }
        SmallHeading(stringResource(R.string.public_name_provider))
        Options(providers.map { it.label }, providers.indexOfFirst { it.key == provider }.coerceAtLeast(0)) {
            provider = providers[it].key; saved = false
        }
        SettingField(stringResource(R.string.public_name_name), name, errors[PublicNameFile.NAME]) { name = it; saved = false }
        Hint(stringResource(R.string.public_name_name_hint))
        SettingField(stringResource(R.string.public_name_token), token, errors[PublicNameFile.TOKEN], secret = true) { token = it; saved = false }
        if (view.tokenSet && token.isEmpty()) Text(stringResource(R.string.public_name_token_saved), color = MaterialTheme.colorScheme.primary)
        Hint(stringResource(R.string.public_name_token_hint))
        Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
            .toggleable(terms, role = Role.Checkbox) { terms = it; saved = false },
            verticalAlignment = Alignment.CenterVertically) {
            Checkbox(terms, onCheckedChange = null, Modifier.padding(end = 12.dp))
            Text(stringResource(R.string.public_name_terms), Modifier.weight(1f))
        }
        TextButton(onClick = {
            runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(context.getString(R.string.public_name_terms_url)))) }
        }) { Text(stringResource(R.string.public_name_terms_url)) }
        Text(publicNameStatusText(context, status, url), Modifier.padding(vertical = 8.dp),
            color = if (status.state == PublicNameState.ERROR) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface)
        if (saved) Text(stringResource(R.string.settings_saved), color = MaterialTheme.colorScheme.primary)
        if (failed) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        Button(enabled = !busy, onClick = {
            val edit = PublicNameEdit(enabled, DnsProviderId.of(provider), name, token, terms)
            busy = true
            scope.launch {
                val problems = save(edit)
                busy = false
                failed = problems == null
                if (problems != null) {
                    errors = problems.associate { it.key to if (it.key == PublicNameFile.NAME) R.string.public_name_name_invalid else R.string.public_name_token_invalid }
                    if (problems.isEmpty()) { token = ""; saved = true }
                }
            }
        }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.public_name_save)) }
        OutlinedButton(onClick = importFile, Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.public_name_import)) }
        OutlinedButton(onClick = renew, Modifier.fillMaxWidth().heightIn(min = 48.dp), enabled = view.configured && view.enabled) {
            Text(stringResource(R.string.public_name_renew))
        }
        TextButton(onClick = { confirmRemove = true }, Modifier.heightIn(min = 48.dp), enabled = view.configured && !busy) {
            Text(stringResource(R.string.public_name_remove), color = MaterialTheme.colorScheme.error)
        }
    }
    if (confirmRemove) AlertDialog(onDismissRequest = { confirmRemove = false },
        title = { Text(stringResource(R.string.public_name_remove)) },
        text = { Text(stringResource(R.string.public_name_remove_confirm)) },
        confirmButton = { TextButton(onClick = {
            confirmRemove = false
            busy = true
            scope.launch { failed = !remove(); busy = false }
        }) { Text(stringResource(R.string.public_name_remove)) } },
        dismissButton = { TextButton(onClick = { confirmRemove = false }) { Text(stringResource(android.R.string.cancel)) } })
}

/** The shared status line wording; [url] is the full head-unit address while the name is ready. */
fun publicNameStatusText(context: Context, status: PublicNameStatus, url: String?): String = when (status.state) {
    PublicNameState.OFF -> context.getString(R.string.public_name_status_off)
    PublicNameState.INCOMPLETE -> context.getString(R.string.public_name_status_incomplete)
    PublicNameState.WAITING_NETWORK -> context.getString(R.string.public_name_status_waiting_network)
    PublicNameState.UPDATING_ADDRESS -> context.getString(R.string.public_name_status_updating_address)
    PublicNameState.REQUESTING_CERTIFICATE -> context.getString(R.string.public_name_status_requesting_certificate)
    PublicNameState.READY -> context.getString(R.string.public_name_status_ready, url ?: "https://${status.name}:8443/",
        status.notAfter?.let { DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(it)) } ?: "—")
    PublicNameState.ERROR -> context.getString(R.string.public_name_status_error, publicNameErrorText(context, status.error, status.detail),
        status.nextAttempt?.let { nextTry(it) } ?: "—")
}

fun publicNameErrorText(context: Context, error: PublicNameError?, detail: String?): String = when (error) {
    PublicNameError.TOKEN_REJECTED -> context.getString(R.string.public_name_error_token_rejected)
    PublicNameError.NAME_NOT_FOUND -> context.getString(R.string.public_name_error_name_not_found)
    PublicNameError.PROVIDER_RATE_LIMITED -> context.getString(R.string.public_name_error_provider_rate_limited)
    PublicNameError.DNS_NOT_UPDATED -> context.getString(R.string.public_name_error_dns_not_updated)
    PublicNameError.ACME_REJECTED -> context.getString(R.string.public_name_error_acme_rejected, detail.orEmpty())
    PublicNameError.ACME_RATE_LIMITED -> context.getString(R.string.public_name_error_acme_rate_limited)
    PublicNameError.NO_ADDRESS -> context.getString(R.string.public_name_error_no_address)
    PublicNameError.PROVIDER_UNREACHABLE, null -> context.getString(R.string.public_name_error_provider_unreachable)
}

/** Today's retries show the time only; later ones the date too. */
private fun nextTry(at: Long): String {
    val day = DateFormat.getDateInstance(DateFormat.SHORT)
    return if (day.format(Date(at)) == day.format(Date())) DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(at))
        else DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(at))
}

/** One line per problem of an imported file: key, description and the stable error code. */
fun publicNameImportProblems(context: Context, errors: List<ConfigError>): String = errors.joinToString("\n") { error ->
    val description = context.getString(when (error.problem) {
        ConfigProblem.MISSING -> R.string.public_name_problem_missing
        ConfigProblem.INVALID -> R.string.public_name_problem_invalid
        ConfigProblem.UNKNOWN_KEY -> R.string.public_name_problem_unknown_key
        ConfigProblem.BAD_VERSION -> R.string.public_name_problem_bad_version
        ConfigProblem.TOO_LARGE -> R.string.public_name_problem_too_large
        ConfigProblem.NOT_JSON -> R.string.public_name_problem_not_json
    })
    // Unknown keys come from the file: keep them to one short line.
    val key = if (error.key == ConfigError.FILE) context.getString(R.string.public_name_file) else error.key.take(64)
    context.getString(R.string.public_name_import_problem, key, description, error.problem.code)
}
