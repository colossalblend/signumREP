package pw.appbank.signum.ui.screens

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.FolderOpen
import androidx.compose.material.icons.outlined.InstallMobile
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.data.catalog.CatalogResolve
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.InfoRow
import pw.appbank.signum.ui.components.Notice
import pw.appbank.signum.ui.components.NoticeTone
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.components.TextAction
import pw.appbank.signum.ui.theme.signumColors
import java.io.File

@Composable
fun SigningScreen(vm: SignumViewModel, goInstalls: () -> Unit) {
    val s by vm.state.collectAsState()
    val c = signumColors
    val ctx = LocalContext.current
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        runCatching {
            ctx.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        // Копирование и разбор — в модели, в фоне: файл бывает в гигабайт.
        vm.pickIpa(ctx.applicationContext, uri)
    }
    val meta = s.signMeta
    val file = s.signPath.takeIf { it.isNotBlank() }?.let(::File)
    val deviceIos = s.selectedDevice?.productVersion
    val tooOld = meta?.minIos != null && deviceIos != null &&
        runCatching { CatalogResolve.compareIos(deviceIos, meta.minIos) < 0 }.getOrDefault(false)

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        PageHeader("Свой файл", "Установка IPA-файла на iPhone по кабелю")

        Panel {
            if (file == null || meta == null) {
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    Box(
                        Modifier
                            .size(56.dp)
                            .clip(RoundedCornerShape(16.dp))
                            .background(c.brand.copy(alpha = 0.12f)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(Icons.Outlined.Description, contentDescription = null, tint = c.brandSoft, modifier = Modifier.size(28.dp))
                    }
                    Spacer(Modifier.height(12.dp))
                    Text("Файл не выбран", color = c.text, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                    Spacer(Modifier.height(4.dp))
                    Text(
                        "Выберите .ipa из «Загрузок», облака или мессенджера.",
                        color = c.muted,
                        fontSize = 13.sp,
                    )
                    Spacer(Modifier.height(14.dp))
                    PrimaryButton(
                        if (s.signBusy) "Копируем…" else "Выбрать файл",
                        onClick = { picker.launch(arrayOf("application/octet-stream", "application/x-itunes-ipa", "*/*")) },
                        loading = s.signBusy,
                        icon = Icons.Outlined.FolderOpen,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            } else {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        Modifier
                            .size(48.dp)
                            .clip(RoundedCornerShape(14.dp))
                            .background(c.brandBrush),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            meta.name.firstOrNull()?.uppercaseChar()?.toString() ?: "A",
                            color = androidx.compose.ui.graphics.Color.White,
                            fontWeight = FontWeight.Bold,
                            fontSize = 20.sp,
                        )
                    }
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(meta.name, color = c.text, fontWeight = FontWeight.Bold, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(file.name.substringAfter('-'), color = c.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    TextAction("Убрать", onClick = vm::clearPickedIpa, color = c.muted)
                }
                Spacer(Modifier.height(10.dp))
                InfoRow("Bundle ID", meta.bundleId)
                meta.version?.let { InfoRow("Версия", it) }
                meta.minIos?.let { InfoRow("Минимальная iOS", it) }
                InfoRow("Размер", formatSize(file.length()))
                Spacer(Modifier.height(12.dp))
                PrimaryButton(
                    "Установить на iPhone",
                    onClick = {
                        vm.queueLocal(s.signPath, sign = false, install = true)
                        goInstalls()
                    },
                    icon = Icons.Outlined.InstallMobile,
                    enabled = !s.signBusy,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                SecondaryButton(
                    "Выбрать другой файл",
                    onClick = { picker.launch(arrayOf("application/octet-stream", "application/x-itunes-ipa", "*/*")) },
                    loading = s.signBusy,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            StatusLine(s.signMessage, error = s.signMessage.isNotBlank() && !s.signBusy && meta == null)
        }

        if (tooOld) {
            Notice(
                title = "Приложению нужна iOS ${meta?.minIos} или новее",
                text = "На подключённом iPhone iOS $deviceIos — установка, скорее всего, завершится ошибкой.",
                tone = NoticeTone.Warning,
            )
        }
        if (s.selectedDevice == null) {
            Notice(
                title = "iPhone не подключён",
                text = "Файл можно выбрать заранее. Перед установкой подключите iPhone кабелем на вкладке «iPhone».",
            )
        }
        Notice(
            title = "Только уже подписанные файлы",
            text = "Android ставит IPA как есть: из App Store (через Signum), подписанные сертификатом разработчика " +
                "или подготовленные на компьютере. Переподпись и настройки IPA (имя, bundle id, мин. iOS) " +
                "доступны в Signum для компьютера.",
        )
    }
}

private fun formatSize(bytes: Long): String = when {
    bytes < 1024 -> "$bytes Б"
    bytes < 1024 * 1024 -> "${bytes / 1024} КБ"
    bytes < 1024L * 1024 * 1024 -> String.format(java.util.Locale.ROOT, "%.1f МБ", bytes / 1048576.0)
    else -> String.format(java.util.Locale.ROOT, "%.2f ГБ", bytes / 1073741824.0)
}
