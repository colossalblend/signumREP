package pw.appbank.signum.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.QrCode2
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.delay
import pw.appbank.signum.ui.ActivationItemView
import pw.appbank.signum.ui.ActivationView
import pw.appbank.signum.ui.theme.signumColors

/**
 * QR активации приложения — как ActivationModal на ПК.
 *
 * Телефон с Signum показывает код, iPhone сканирует его камерой (или внутри
 * самого приложения). Живые коды tb.php действуют ограниченное время: как
 * только срок вышел, просим у сервера новый.
 */
@Composable
fun ActivationSheet(
    view: ActivationView,
    onRefresh: (force: Boolean) -> Unit,
    onClose: () -> Unit,
) {
    val c = signumColors
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    // Секундомер нужен только живым кодам; цикл снимается вместе с окном.
    val hasTimers = view.items.any { it.expiresAt != null }
    LaunchedEffect(hasTimers) {
        while (hasTimers) {
            now = System.currentTimeMillis()
            delay(1000)
        }
    }
    // Срок вышел — обновляем сами, один раз на каждый истёкший набор кодов.
    val anyExpired = view.items.any { it.live && it.expiresAt != null && it.expiresAt <= now }
    LaunchedEffect(anyExpired, view.loading) {
        if (anyExpired && !view.loading) onRefresh(true)
    }
    val title = view.source.title?.trim()?.takeIf { it.isNotEmpty() } ?: "Активация после установки"
    val message = view.source.message?.trim()?.takeIf { it.isNotEmpty() }
        ?: "Для входа нужно активировать приложение. Отсканируйте QR камерой iPhone."
    val minTtl = view.items.mapNotNull { it.expiresAt }.minOrNull()?.let { ((it - now) / 1000).coerceAtLeast(0) }

    Dialog(onDismissRequest = onClose, properties = DialogProperties(usePlatformDefaultWidth = true)) {
        val shape = RoundedCornerShape(24.dp)
        Column(
            Modifier
                .fillMaxWidth()
                .clip(shape)
                .background(c.surfaceStrong)
                .border(1.dp, c.border, shape)
                .verticalScroll(rememberScrollState())
                .padding(18.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).background(c.brand.copy(alpha = 0.14f)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Outlined.QrCode2, contentDescription = null, tint = c.brandSoft, modifier = Modifier.size(22.dp))
                }
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(title, color = c.text, fontSize = 17.sp, fontWeight = FontWeight.Bold)
                    val sub = buildString {
                        append(view.appTitle)
                        if (view.items.size == 1) append(" · ").append(view.items.first().item.title)
                        else append(" · ").append(view.items.size).append(" QR")
                        if (view.source.provider == "tb") append(" · код на 10 минут")
                    }
                    Text(sub, color = c.muted, fontSize = 12.sp, maxLines = 2)
                }
            }
            Spacer(Modifier.height(12.dp))
            Text(message, color = c.text, fontSize = 13.sp, lineHeight = 19.sp)
            if (minTtl != null && !view.loading) {
                Spacer(Modifier.height(4.dp))
                Text("Следующая смена QR через ${formatTtl(minTtl)}", color = c.muted, fontSize = 11.sp)
            }
            StatusLine(view.error, error = true)
            Spacer(Modifier.height(12.dp))
            if (view.loading && view.items.none { it.payload.isNotEmpty() }) {
                Column(
                    Modifier.fillMaxWidth().padding(vertical = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    CircularProgressIndicator(color = c.brandSoft, strokeWidth = 2.5.dp, modifier = Modifier.size(28.dp))
                    Spacer(Modifier.height(10.dp))
                    Text("Готовим QR-код…", color = c.muted, fontSize = 13.sp)
                }
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    view.items.forEach { QrBlock(it, now) }
                }
            }
            Spacer(Modifier.height(14.dp))
            if (view.items.any { it.live }) {
                SecondaryButton(
                    "Обновить QR",
                    onClick = { onRefresh(true) },
                    icon = Icons.Outlined.Refresh,
                    loading = view.loading,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
            }
            SecondaryButton("Закрыть", onClick = onClose, modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun QrBlock(entry: ActivationItemView, now: Long) {
    val c = signumColors
    val copy = rememberCopyText()
    var copied by remember(entry.payload) { mutableStateOf(false) }
    LaunchedEffect(copied) {
        if (copied) {
            delay(1600)
            copied = false
        }
    }
    val ttlLeft = entry.expiresAt?.let { ((it - now) / 1000).coerceAtLeast(0) }
    val expired = ttlLeft != null && ttlLeft <= 0
    val payload = entry.payload
    val qr = remember(payload) { if (payload.isEmpty()) null else qrBitmap(payload) }
    val shape = RoundedCornerShape(16.dp)
    Column(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.inset)
            .border(1.dp, c.border, shape)
            .padding(14.dp),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f)) {
                Text(entry.item.title.ifBlank { "QR-код" }, color = c.text, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
                entry.item.subtitle?.takeIf { it.isNotBlank() }?.let {
                    Text(it, color = c.muted, fontSize = 11.sp)
                }
            }
            Spacer(Modifier.width(8.dp))
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                StatusChip(if (entry.inApp) "В приложении" else "Камера iPhone", if (entry.inApp) Color(0xFFA78BFA) else Color(0xFF38BDF8))
                if (ttlLeft != null) {
                    StatusChip(
                        if (expired) "истёк" else "ещё ${formatTtl(ttlLeft)}",
                        when {
                            expired -> c.danger
                            ttlLeft < 60 -> c.warning
                            else -> c.muted
                        },
                    )
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(
            if (expired) "Срок QR истёк — сейчас появится новый код." else entry.hint,
            color = c.muted,
            fontSize = 12.sp,
            lineHeight = 17.sp,
        )
        Spacer(Modifier.height(10.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 120.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(Color.White)
                .padding(12.dp)
                .alpha(if (expired) 0.35f else 1f),
            contentAlignment = Alignment.Center,
        ) {
            if (qr != null) {
                Image(qr, contentDescription = "QR-код активации ${entry.item.title}", Modifier.size(220.dp))
            } else {
                Text(
                    if (payload.isEmpty()) "QR ещё не сгенерирован." else "Не удалось построить QR. Скопируйте данные вручную.",
                    color = Color(0xFF475569),
                    fontSize = 12.sp,
                    textAlign = TextAlign.Center,
                )
            }
        }
        if (payload.isNotEmpty() && !expired) {
            Spacer(Modifier.height(8.dp))
            Text(
                payload,
                color = c.muted,
                fontSize = 10.sp,
                lineHeight = 14.sp,
                fontFamily = FontFamily.Monospace,
                maxLines = 3,
            )
        }
        Spacer(Modifier.height(8.dp))
        SecondaryButton(
            when {
                copied -> "Скопировано"
                entry.inApp -> "Скопировать данные QR"
                else -> "Скопировать ссылку"
            },
            onClick = {
                copy(payload)
                copied = true
            },
            icon = Icons.Outlined.ContentCopy,
            enabled = payload.isNotEmpty() && !expired,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

private fun formatTtl(seconds: Long): String {
    val s = seconds.coerceAtLeast(0)
    return "${s / 60}:${(s % 60).toString().padStart(2, '0')}"
}
