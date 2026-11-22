package pw.appbank.signum.ui.components

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color as AndroidColor
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import pw.appbank.signum.ui.theme.signumColors

/**
 * Установка приложения, которое раздаётся через TestFlight.
 *
 * Такие приложения нельзя ни купить, ни скачать как IPA — Apple отдаёт их
 * только по приглашению, и открыть его должен сам iPhone. Поэтому здесь нет
 * кнопки «установить»: телефон сканирует код с экрана Android либо получает
 * ссылку любым мессенджером.
 */
@Composable
fun TestFlightSheet(
    title: String,
    inviteUrl: String,
    testFlightOnPhone: Boolean?,
    onClose: () -> Unit,
) {
    val c = signumColors
    val ctx = LocalContext.current
    // Код не меняется, пока открыта карточка, — строим один раз.
    val qr = remember(inviteUrl) { qrBitmap(inviteUrl) }

    Dialog(onDismissRequest = onClose) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(20.dp))
                .background(c.surfaceStrong)
                .verticalScroll(rememberScrollState())
                .padding(18.dp),
        ) {
            Text(title, color = c.text, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(2.dp))
            Text("Установка через TestFlight", color = c.muted, fontSize = 12.sp)

            if (testFlightOnPhone == false) {
                Spacer(Modifier.height(12.dp))
                Box(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(12.dp))
                        .background(Color(0x1AF5B72F))
                        .padding(10.dp),
                ) {
                    Text(
                        "На iPhone нет TestFlight. Установите его из App Store — это бесплатное " +
                            "приложение Apple, без него приглашение не откроется.",
                        color = Color(0xFFFCD34D),
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                    )
                }
            }

            Spacer(Modifier.height(14.dp))
            if (qr != null) {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(14.dp))
                        .background(Color.White)
                        .padding(12.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Image(qr, contentDescription = "QR-код приглашения TestFlight", Modifier.size(230.dp))
                }
            }

            Spacer(Modifier.height(14.dp))
            listOf(
                "Наведите камеру iPhone на этот код.",
                "Откройте всплывшую ссылку — запустится TestFlight.",
                "Нажмите «Принять», затем «Установить».",
                "Готово — $title появится на экране «Домой».",
            ).forEachIndexed { index, step ->
                Row(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
                    Text(
                        "${index + 1}",
                        color = c.brandSoft,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.size(16.dp),
                    )
                    Text(step, color = c.text, fontSize = 12.sp, lineHeight = 17.sp)
                }
            }

            Spacer(Modifier.height(12.dp))
            Text(inviteUrl, color = c.muted, fontSize = 11.sp, lineHeight = 15.sp)

            Spacer(Modifier.height(12.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton(
                    "Копировать",
                    onClick = { copyToClipboard(ctx, title, inviteUrl) },
                    modifier = Modifier.weight(1f),
                )
                SecondaryButton(
                    "Поделиться",
                    onClick = { shareLink(ctx, inviteUrl) },
                    modifier = Modifier.weight(1f),
                )
            }
            Spacer(Modifier.height(8.dp))
            SecondaryButton("Закрыть", onClick = onClose, modifier = Modifier.fillMaxWidth())
        }
    }
}

private fun copyToClipboard(ctx: Context, label: String, text: String) {
    val clip = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
    clip.setPrimaryClip(ClipData.newPlainText(label, text))
}

private fun shareLink(ctx: Context, url: String) {
    val intent = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_TEXT, url)
    }
    runCatching { ctx.startActivity(Intent.createChooser(intent, "Отправить ссылку")) }
}

/**
 * QR из ссылки-приглашения.
 *
 * Уровень коррекции M, а не H: ссылки TestFlight короткие, запаса хватает, а
 * модули остаются крупными — камера телефона ловит код с экрана увереннее.
 */
private fun qrBitmap(text: String, size: Int = 512): ImageBitmap? = runCatching {
    val hints = mapOf(
        EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M,
        EncodeHintType.MARGIN to 1,
        EncodeHintType.CHARACTER_SET to "UTF-8",
    )
    val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size, hints)
    val pixels = IntArray(matrix.width * matrix.height)
    for (y in 0 until matrix.height) {
        val row = y * matrix.width
        for (x in 0 until matrix.width) {
            pixels[row + x] = if (matrix.get(x, y)) AndroidColor.BLACK else AndroidColor.WHITE
        }
    }
    Bitmap.createBitmap(pixels, matrix.width, matrix.height, Bitmap.Config.ARGB_8888).asImageBitmap()
}.getOrNull()
