// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui.design

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.MotionDurationScale
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.*
import kotlin.math.abs

@Stable
@OptIn(ExperimentalMaterial3Api::class)
class StagePageState internal constructor(
    val scaffoldState: BottomSheetScaffoldState,
    val listState: LazyListState,
    private val motion: State<Boolean>,
) {
    val isExpanded: Boolean get() = scaffoldState.bottomSheetState.let {
        it.currentValue == SheetValue.Expanded || it.targetValue == SheetValue.Expanded
    }

    suspend fun expand() = move(true)
    suspend fun collapse() = move(false)

    suspend fun openSection(section: PathSection, itemKeys: List<Any>, sectionKey: (PathSection) -> Any = { it }) {
        val index = itemKeys.indexOf(sectionKey(section))
        expand()
        if (index >= 0) listState.scrollToItem(index)
    }

    private suspend fun move(expand: Boolean) {
        suspend fun change() {
            if (expand) scaffoldState.bottomSheetState.expand() else scaffoldState.bottomSheetState.partialExpand()
        }
        if (motion.value) change() else withContext(object : MotionDurationScale {
            override val scaleFactor = 0f
        }) { change() }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun rememberStagePageState(): StagePageState {
    val scaffold = rememberBottomSheetScaffoldState()
    val list = rememberLazyListState()
    val motion = rememberUpdatedState(LocalRelayMotion.current)
    return remember(scaffold, list) { StagePageState(scaffold, list, motion) }
}

/** [onNode] expands the caller's section. [drawerItemKeys] lists all lazy keys in order, using [sectionKey] for sections. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun StagePage(path: ConnectionPath, view: StageView,
    onAction: (StageAction) -> Unit, onCopy: (String) -> Unit, onNotice: (StageNotice) -> Unit,
    onCountdownComplete: () -> Unit, modifier: Modifier = Modifier,
    state: StagePageState = rememberStagePageState(),
    topBar: @Composable RowScope.() -> Unit = {},
    onNode: (PathSection) -> Unit = {},
    onDrawerOpened: () -> Unit = {},
    drawerItemKeys: List<Any> = view.segments.map { it.section },
    sectionKey: (PathSection) -> Any = { it },
    highPerformance: Boolean = false, onTouch: () -> Unit = {}, manageWindow: Boolean = true,
    drawerContent: LazyListScope.() -> Unit,
) {
    val minimal by rememberUpdatedState(highPerformance)
    val touch by rememberUpdatedState(onTouch)
    var arrived by rememberSaveable { mutableStateOf(highPerformance) }
    LaunchedEffect(Unit) { arrived = true }
    // Keep this input node through wake-up so the entire first gesture stays consumed.
    Box(Modifier.fillMaxSize().pointerInput(Unit) {
        awaitPointerEventScope {
            var waking = false
            while (true) {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                if (event.changes.any { it.pressed && !it.previousPressed }) {
                    if (minimal) waking = true
                    touch()
                }
                if (waking) event.changes.forEach { it.consume() }
                if (event.changes.none { it.pressed }) waking = false
            }
        }
    }) {
        if (highPerformance) StageMinimal(onTouch, manageWindow)
        else StagePageContent(path, view, onAction, onCopy, onNotice, onCountdownComplete, modifier, state,
            !arrived, topBar, onNode, onDrawerOpened, drawerItemKeys, sectionKey, drawerContent)
    }
    BackHandler(highPerformance) { onTouch() }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable private fun StagePageContent(path: ConnectionPath, view: StageView,
    onAction: (StageAction) -> Unit, onCopy: (String) -> Unit, onNotice: (StageNotice) -> Unit,
    onCountdownComplete: () -> Unit, modifier: Modifier, state: StagePageState, arrivalEnabled: Boolean,
    topBar: @Composable RowScope.() -> Unit, onNode: (PathSection) -> Unit, onDrawerOpened: () -> Unit,
    drawerItemKeys: List<Any>, sectionKey: (PathSection) -> Any, drawerContent: LazyListScope.() -> Unit) {
    val motion = LocalRelayMotion.current
    val scope = rememberCoroutineScope()
    val sheet = state.scaffoldState.bottomSheetState
    val countdownActive = view.countdown && view.primary == StageAction.Connect && view.primaryEnabled
    val deadline = rememberSaveable(countdownActive) {
        if (countdownActive) System.currentTimeMillis() + STAGE_COUNTDOWN_SECONDS * 1000L else 0L
    }
    LaunchedEffect(motion) {
        if (!motion) {
            if (sheet.targetValue == SheetValue.Expanded) state.expand() else state.collapse()
        }
    }
    val expanded by remember(state) { derivedStateOf { state.isExpanded } }
    val opened by rememberUpdatedState(onDrawerOpened)
    LaunchedEffect(sheet) {
        snapshotFlow { sheet.targetValue == SheetValue.Expanded }.collect { if (it) opened() }
    }
    if (Build.VERSION.SDK_INT >= 34) {
        // SheetState exposes no public offset setter; a cancelled gesture leaves it open.
        PredictiveBackHandler(expanded) { progress ->
            progress.collect { }
            state.collapse()
        }
    } else BackHandler(expanded) { scope.launch { state.collapse() } }
    val navigation = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val peek = 72.dp + navigation
    val background = MaterialTheme.colorScheme.background
    val scrim = MaterialTheme.colorScheme.scrim
    CompositionLocalProvider(LocalStageArrivalEnabled provides arrivalEnabled,
        LocalStageCountdownDeadline provides deadline.takeIf { countdownActive }) {
        BoxWithConstraints(modifier.fillMaxSize().background(background)) {
            val height = maxHeight
            val density = LocalDensity.current
            val travel = with(density) { (height - peek).toPx().coerceAtLeast(1f) }
            val progress: () -> Float = {
                val offset = runCatching { sheet.requireOffset() }.getOrDefault(travel)
                ((travel - offset) / travel).coerceIn(0f, 1f)
            }
            val node: (PathSection) -> Unit = { section ->
                onNode(section)
                scope.launch { state.openSection(section, drawerItemKeys, sectionKey) }
            }
            val stage: @Composable (StageLayout) -> Unit = { layout ->
                ConnectionStage(path, view.nodes, Modifier.graphicsLayer {
                    val scale = 1f - .06f * progress()
                    scaleX = scale
                    scaleY = scale
                }, layout, node)
            }
            BottomSheetScaffold(scaffoldState = state.scaffoldState, sheetPeekHeight = peek,
                sheetMaxWidth = 720.dp, sheetShape = RelayTokens.sheet,
                sheetContainerColor = MaterialTheme.colorScheme.surface,
                sheetSwipeEnabled = motion,
                sheetDragHandle = {
                    val snapDrag = if (motion) Modifier else Modifier.pointerInput(state) {
                        var distance = 0f
                        detectVerticalDragGestures(onDragStart = { distance = 0f }, onDragEnd = {
                            if (abs(distance) > 24.dp.toPx()) {
                                val open = distance < 0f
                                scope.launch { if (open) state.expand() else state.collapse() }
                            }
                        }) { change, amount -> change.consume(); distance += amount }
                    }
                    DrawerHandle(view, progress, {
                        scope.launch { if (expanded) state.collapse() else state.expand() }
                    }, Modifier.padding(bottom = navigation).then(snapDrag), expanded)
                }, sheetContent = {
                    // Dark grouped cards sit one tone above the sheet, as on iOS; light cards keep their outline.
                    val scheme = MaterialTheme.colorScheme
                    val cards = if (LocalRelayDark.current) scheme.copy(surface = scheme.surfaceContainerHigh,
                        outlineVariant = Color(0xFF48484A)) else scheme
                    // A closed drawer composes nothing, so Home updates and cold start skip the hidden steps;
                    // the fixed height keeps the sheet's anchors unchanged.
                    val shown by remember(sheet, travel) { derivedStateOf {
                        sheet.targetValue == SheetValue.Expanded || progress() > 0f
                    } }
                    Box(Modifier.fillMaxWidth().height((height - peek).coerceAtLeast(0.dp))) {
                        if (shown) MaterialTheme(colorScheme = cards, typography = MaterialTheme.typography, shapes = MaterialTheme.shapes) {
                            LazyColumn(Modifier.fillMaxSize(), state = state.listState,
                                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp + navigation),
                                verticalArrangement = Arrangement.spacedBy(12.dp), content = drawerContent)
                        }
                    }
                }, containerColor = background) { padding ->
                Column(Modifier.fillMaxSize().padding(padding).drawWithContent {
                    drawContent()
                    drawRect(scrim.copy(alpha = .32f * progress()))
                }) {
                    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 12.dp),
                        verticalAlignment = Alignment.CenterVertically, content = topBar)
                    BoxWithConstraints(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 24.dp)) {
                        val layout = stageLayout(height, maxHeight)
                        if (height < 480.dp) {
                            Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(24.dp),
                                verticalAlignment = Alignment.CenterVertically) {
                                Box(Modifier.weight(1f), contentAlignment = Alignment.Center) { stage(layout) }
                                Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(vertical = 12.dp),
                                    verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.Start) {
                                    StageHeadline(view, onCopy, textAlign = TextAlign.Start)
                                    NoticeChip(view.notice, onNotice)
                                    StageActions(view, onAction, onCountdownComplete, sideBySide = true)
                                }
                            }
                        } else StagePortrait(maxHeight, { stage(layout) }, {
                            StageHeadline(view, onCopy)
                            NoticeChip(view.notice, onNotice)
                        }, { StageActions(view, onAction, onCountdownComplete) })
                    }
                }
            }
        }
    }
}

@Composable private fun StagePortrait(height: androidx.compose.ui.unit.Dp, stage: @Composable () -> Unit,
    text: @Composable () -> Unit, actions: @Composable () -> Unit) {
    Layout(modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).heightIn(min = height), content = {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
            stage()
            text()
        }
        Box { actions() }
    }) { measurables, constraints ->
        val loose = constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity)
        val main = measurables[0].measure(loose)
        val buttons = measurables[1].measure(loose)
        val margin = 16.dp.roundToPx()
        val gap = 24.dp.roundToPx()
        val fullHeight = maxOf(constraints.minHeight, main.height + buttons.height + margin * 2 + gap)
        val free = (fullHeight - main.height - buttons.height - margin * 2 - gap).coerceAtLeast(0)
        layout(constraints.maxWidth, fullHeight) {
            main.placeRelative(0, margin + (free / 2.2f).toInt())
            buttons.placeRelative(0, fullHeight - margin - buttons.height)
        }
    }
}

@Composable fun DrawerHandle(view: StageView, progress: () -> Float, onClick: () -> Unit,
    modifier: Modifier = Modifier, expanded: Boolean = false) {
    val label = stringResource(R.string.stage_steps)
    val description = stringResource(R.string.stage_steps_progress, view.done, view.total)
    val colors = LocalStatusColors.current
    val track = MaterialTheme.colorScheme.outlineVariant
    val motion = LocalRelayMotion.current
    val closed by remember(progress, expanded) { derivedStateOf { !expanded && progress() <= 0f } }
    val nudge = stageDrawerNudge(closed)
    Column(modifier.fillMaxWidth().heightIn(min = 72.dp).semantics(mergeDescendants = true) { contentDescription = description }
        .clickable(role = Role.Button, onClickLabel = label, onClick = onClick).padding(horizontal = 24.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterVertically)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            view.segments.forEach { segment ->
                val fill = stageFloat(if (segment.total > 0) segment.done.toFloat() / segment.total else 0f)
                Canvas(Modifier.weight(segment.total.coerceAtLeast(1).toFloat()).height(4.dp)) {
                    drawLine(track, Offset(0f, center.y), Offset(size.width, center.y), size.height, StrokeCap.Round)
                    if (fill.value > 0f) drawLine(colors.color(segment.state), Offset(0f, center.y),
                        Offset(size.width * fill.value.coerceIn(0f, 1f), center.y), size.height, StrokeCap.Round)
                }
            }
        }
        Row(Modifier.fillMaxWidth().graphicsLayer { translationY = if (motion && closed) nudge.value * density else 0f },
            horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.stage_swipe_hint), Modifier.weight(1f).graphicsLayer {
                alpha = (1f - 2f * progress()).coerceIn(0f, 1f)
            }.clearAndSetSemantics {}, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
            Text(stringResource(R.string.stage_progress_count, view.done, view.total), Modifier.clearAndSetSemantics {},
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelMedium.copy(fontFeatureSettings = "tnum"))
            Symbol(R.drawable.symbol_chevron_down, null, Modifier.size(20.dp).graphicsLayer { rotationZ = 180f * (1f - progress()) })
        }
    }
}
