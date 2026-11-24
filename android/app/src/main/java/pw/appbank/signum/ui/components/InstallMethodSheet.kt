package pw.appbank.signum.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.Usb
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.data.models.DeviceInfo
import pw.appbank.signum.ui.theme.signumColors

/**
 * Красивый выбор способа установки на iPhone: USB/Wi-Fi или Wi-Fi сервер.
 * Показывается перед постановкой установки в очередь. Устройство найдено —
 * кабель уже под рукой; не найдено — Wi-Fi сервер. Способ всегда можно
 * переключить вручную.
 */
@Composable
fun InstallMethodSheet(
    devices: List<DeviceInfo>,
    detecting: Boolean,
    onPick: (method: String) -> Unit,
    onCancel: () -> Unit,
) {
    val c = signumColors
    val device = devices.firstOrNull()
    val found = device != null
    Box(
        Modifier
            .fillMaxSize()
            .background(Color(0x88000000))
            .clickable(enabled = false) {},
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier
                .padding(22.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(22.dp))
                .background(c.surface)
                .padding(20.dp),
        ) {
            Text(
                "Установка на iPhone",
                color = c.text,
                fontWeight = FontWeight.Bold,
                fontSize = 17.sp,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                when {
                    detecting -> "Ищем iPhone…"
                    found -> "iPhone найден — выберите, как поставить приложение."
                    else -> "iPhone не найден — доступен Wi-Fi сервер, кабель не нужен."
                },
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 16.sp,
            )
            Spacer(Modifier.height(14.dp))
            MethodCard(
                title = "USB / Wi-Fi",
                hint = when {
                    detecting -> "Проверяем подключение…"
                    found -> (device.deviceName ?: "iPhone") + " · " + connectionLabel(device.connection)
                    else -> "iPhone не найден — подключите кабель и попробуйте снова"
                },
                recommended = found && !detecting,
                badge = if (found && !detecting) "iPhone найден" else "",
                icon = {
                    Icon(Icons.Filled.Usb, contentDescription = null, tint = c.brand, modifier = Modifier.size(20.dp))
                },
                onClick = { onPick("usb") },
            )
            Spacer(Modifier.height(10.dp))
            MethodCard(
                title = "Wi-Fi сервер",
                hint = "QR-код и ссылка: iPhone скачает и установит приложение сам",
                recommended = !found && !detecting,
                badge = if (!found && !detecting) "Кабель не нужен" else "",
                icon = {
                    Icon(Icons.Filled.QrCode2, contentDescription = null, tint = c.brand, modifier = Modifier.size(20.dp))
                },
                onClick = { onPick("server") },
            )
            Spacer(Modifier.height(14.dp))
            Text(
                "Отмена",
                color = c.muted,
                fontSize = 13.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .align(Alignment.CenterHorizontally)
                    .clip(RoundedCornerShape(10.dp))
                    .clickable(onClick = onCancel)
                    .padding(horizontal = 14.dp, vertical = 8.dp),
            )
        }
    }
}

private fun connectionLabel(connection: String): String = when (connection.lowercase()) {
    "wifi" -> "Wi-Fi"
    "both" -> "USB + Wi-Fi"
    else -> "USB"
}

@Composable
private fun MethodCard(
    title: String,
    hint: String,
    recommended: Boolean,
    badge: String,
    icon: @Composable () -> Unit,
    onClick: () -> Unit,
) {
    val c = signumColors
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(if (recommended) c.brandSoft else c.inset)
            .border(1.dp, if (recommended) c.brand else c.border, RoundedCornerShape(16.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            icon()
            Spacer(Modifier.width(8.dp))
            Text(title, color = c.text, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            if (badge.isNotEmpty()) {
                Spacer(Modifier.weight(1f))
                Text(
                    badge,
                    color = if (recommended) c.brand else c.muted,
                    fontSize = 10.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
        }
        Spacer(Modifier.height(4.dp))
        Text(hint, color = c.muted, fontSize = 11.sp, lineHeight = 15.sp)
    }
}
