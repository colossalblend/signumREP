package pw.appbank.signum.ui.components

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
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
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import pw.appbank.signum.data.device.InstallServer
import pw.appbank.signum.ui.theme.signumColors

/**
 * Оверлей «Wi-Fi сервер установки»: QR-код, ссылка, подсказка и кнопка
 * остановки сервера. Показывается, пока сервер жив (метод «server»).
 */
@Composable
fun WifiServerSheet(
    serving: InstallServer.Serving,
    onStop: () -> Unit,
) {
    val c = signumColors
    val ctx = LocalContext.current
    Box(
        Modifier
            .fillMaxSize()
            .background(androidx.compose.ui.graphics.Color(0x88000000))
            .clickable(enabled = false) {},
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier
                .padding(22.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(22.dp))
                .background(c.surface)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Wi-Fi сервер установки",
                    color = c.text,
                    fontWeight = FontWeight.Bold,
                    fontSize = 17.sp,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = onStop) {
                    Icon(Icons.Filled.Close, contentDescription = "Остановить сервер", tint = c.muted)
                }
            }
            Spacer(Modifier.height(10.dp))
            Text(
                serving.title,
                color = c.text,
                fontWeight = FontWeight.SemiBold,
                fontSize = 15.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                "версия ${serving.version} · ${serving.ipa.length() / 1_048_576} МБ",
                color = c.muted,
                fontSize = 12.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(16.dp))
            QrImage(content = serving.url, sizeDp = 232.dp)
            Spacer(Modifier.height(16.dp))
            Text(
                "Откройте камеру iPhone и наведите на код, или введите ссылку в Safari:",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            Row(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(c.inset)
                    .clickable {
                        val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        cm.setPrimaryClip(ClipData.newPlainText("Signum", serving.url))
                    }
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    serving.url,
                    color = c.text,
                    fontSize = 13.sp,
                    modifier = Modifier.weight(1f),
                )
                Icon(Icons.Filled.ContentCopy, contentDescription = "Скопировать", tint = c.muted, modifier = Modifier.size(16.dp))
            }
            Spacer(Modifier.height(14.dp))
            Text(
                "Не закрывайте Signum и не отключайте Wi-Fi, пока iPhone скачивает приложение.",
                color = c.muted,
                fontSize = 11.sp,
                lineHeight = 15.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun QrImage(content: String, sizeDp: androidx.compose.ui.unit.Dp) {
    val qr = remember(content) { qrBitmap(content) }
    qr?.let {
        Image(
            bitmap = it.asImageBitmap(),
            contentDescription = "QR-код со ссылкой",
            modifier = Modifier
                .size(sizeDp)
                .clip(RoundedCornerShape(18.dp))
                .background(androidx.compose.ui.graphics.Color.White)
                .padding(10.dp),
        )
    }
}

private fun qrBitmap(content: String): Bitmap? {
    return runCatching {
        val hints = mapOf(EncodeHintType.MARGIN to 1, EncodeHintType.CHARACTER_SET to "UTF-8")
        val matrix = QRCodeWriter().encode(content, BarcodeFormat.QR_CODE, 512, 512, hints)
        val bmp = Bitmap.createBitmap(matrix.width, matrix.height, Bitmap.Config.ARGB_8888)
        for (y in 0 until matrix.height) {
            for (x in 0 until matrix.width) {
                bmp.setPixel(x, y, if (matrix[x, y]) Color.BLACK else Color.WHITE)
            }
        }
        bmp
    }.getOrNull()
}
