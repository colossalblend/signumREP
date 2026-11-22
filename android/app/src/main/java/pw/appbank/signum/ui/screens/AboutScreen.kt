package pw.appbank.signum.ui.screens

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.Code
import androidx.compose.material.icons.outlined.Language
import androidx.compose.material.icons.outlined.PhoneAndroid
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.appbank.signum.R
import pw.appbank.signum.data.models.APP_BUILD
import pw.appbank.signum.data.models.APP_VERSION
import pw.appbank.signum.ui.components.NavRow
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.RowDivider
import pw.appbank.signum.ui.components.SectionHeader
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.components.TextAction
import pw.appbank.signum.ui.theme.signumColors

private const val TELEGRAM_URL = "https://t.me/appbankRU"
private const val SITE_URL = "https://appbank.pw"
private const val IPA_DOWNLOADER_URL = "https://github.com/kda2495/IPA_Downloader"

private data class Library(val name: String, val role: String, val url: String)

/**
 * Что лежит под капотом Android-версии. Порядок — от того, что видит
 * пользователь, к инфраструктуре; список свой, не настольный: здесь нет ни
 * Tauri, ни libimobiledevice.
 */
private val LIBRARY_GROUPS = listOf(
    "Apple ID и App Store" to listOf(
        Library("ipatool", "Протокол входа, покупки и скачивания IPA", "https://github.com/majd/ipatool"),
        Library("sapsigner", "FairPlay и подпись запросов к App Store", "https://github.com/t0rr3sp3dr0/sapsigner"),
        Library("Unicorn Engine", "Эмуляция для sapsigner", "https://github.com/unicorn-engine/unicorn"),
    ),
    "iPhone по USB" to listOf(
        Library("libimobiledevice", "Протоколы usbmux, lockdown и установки — образец для Android-версии", "https://github.com/libimobiledevice/libimobiledevice"),
        Library("Bouncy Castle", "TLS-сопряжение с iPhone и сертификаты", "https://github.com/bcgit/bc-java"),
        Library("dd-plist", "Чтение и запись plist", "https://github.com/3breadt/dd-plist"),
    ),
    "Приложение" to listOf(
        Library("Jetpack Compose", "Интерфейс", "https://developer.android.com/compose"),
        Library("OkHttp", "Сеть и прокси", "https://github.com/square/okhttp"),
        Library("Coil", "Значки приложений", "https://github.com/coil-kt/coil"),
        Library("ZXing", "QR-коды", "https://github.com/zxing/zxing"),
        Library("kotlinx.serialization", "Разбор ответов сервера", "https://github.com/Kotlin/kotlinx.serialization"),
    ),
)

@Composable
fun AboutScreen(onBack: () -> Unit) {
    val c = signumColors
    val ctx = LocalContext.current
    var linkError by remember { mutableStateOf("") }
    fun open(url: String) {
        linkError = openExternal(ctx, url) ?: ""
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = null, tint = c.muted, modifier = Modifier.size(16.dp))
            TextAction("Настройки", onClick = onBack, color = c.muted)
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Image(
                painterResource(R.drawable.signum_icon),
                contentDescription = null,
                modifier = Modifier.size(64.dp).clip(RoundedCornerShape(18.dp)),
            )
            Spacer(Modifier.width(14.dp))
            Column {
                Text("Signum", color = c.text, fontSize = 24.sp, fontWeight = FontWeight.Bold)
                Text("Установка приложений на iPhone с Android", color = c.muted, fontSize = 13.sp)
                Text("$APP_VERSION · build $APP_BUILD", color = c.muted.copy(alpha = 0.75f), fontSize = 11.sp)
            }
        }

        Panel {
            NavRow(
                icon = Icons.AutoMirrored.Outlined.Send,
                title = "Telegram-канал",
                subtitle = "@appbankRU — новости и помощь",
                trailingIcon = Icons.AutoMirrored.Outlined.OpenInNew,
                onClick = { open(TELEGRAM_URL) },
            )
            RowDivider()
            NavRow(
                icon = Icons.Outlined.Language,
                title = "Сайт",
                subtitle = "appbank.pw — загрузки для всех систем",
                trailingIcon = Icons.AutoMirrored.Outlined.OpenInNew,
                onClick = { open(SITE_URL) },
            )
            StatusLine(linkError, error = true)
        }

        Panel {
            SectionHeader("В основе", Modifier.padding(bottom = 4.dp))
            NavRow(
                icon = Icons.Outlined.Code,
                title = "IPA Downloader",
                subtitle = "Signum вырос из IPA Downloader: оттуда пришла идея ставить нужные версии приложений без джейлбрейка.",
                trailingIcon = Icons.AutoMirrored.Outlined.OpenInNew,
                onClick = { open(IPA_DOWNLOADER_URL) },
            )
        }

        Column {
            Text("Библиотеки и компоненты", color = c.text, fontSize = 15.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(2.dp))
            Text(
                "Открытые проекты, без которых Signum не работал бы. Спасибо их авторам.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
        }
        LIBRARY_GROUPS.forEach { (title, items) ->
            SectionHeader(title)
            Panel {
                items.forEachIndexed { index, lib ->
                    if (index > 0) RowDivider()
                    LibraryRow(lib) { open(lib.url) }
                }
            }
        }

        Panel {
            NavRow(
                icon = Icons.Outlined.PhoneAndroid,
                title = "Signum в настройках Android",
                subtitle = "Разрешения, очистка данных и удаление приложения",
                trailingIcon = Icons.AutoMirrored.Outlined.OpenInNew,
                onClick = { linkError = openAppDetails(ctx) ?: "" },
            )
        }

        Text(
            "Signum не связан с Apple Inc. App Store, iPhone и Apple ID — товарные знаки Apple Inc.",
            color = c.muted.copy(alpha = 0.7f),
            fontSize = 11.sp,
            lineHeight = 16.sp,
            textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth().padding(vertical = 12.dp),
        )
    }
}

@Composable
private fun LibraryRow(lib: Library, onClick: () -> Unit) {
    val c = signumColors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(lib.name, color = c.text, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            Text(lib.role, color = c.muted, fontSize = 12.sp, lineHeight = 16.sp)
        }
        Spacer(Modifier.width(8.dp))
        Icon(Icons.AutoMirrored.Outlined.OpenInNew, contentDescription = null, tint = c.muted, modifier = Modifier.size(16.dp))
    }
}

/** Открыть ссылку во внешнем приложении. Возвращает текст ошибки или null. */
private fun openExternal(ctx: Context, url: String): String? = try {
    ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    null
} catch (_: ActivityNotFoundException) {
    "Не нашлось приложения, чтобы открыть ссылку: $url"
} catch (e: Exception) {
    "Не удалось открыть ссылку: ${e.message ?: url}"
}

/**
 * Карточка Signum в системных настройках. Удаляет приложение и стирает его
 * данные на Android сама система — своё окно удаления, как на ПК, здесь не нужно.
 */
private fun openAppDetails(ctx: Context): String? = try {
    ctx.startActivity(
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + ctx.packageName))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
    )
    null
} catch (_: Exception) {
    "Не удалось открыть настройки Android."
}
