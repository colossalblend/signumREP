package pw.appbank.signum.ui.screens

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.theme.signumColors
import java.io.File

@Composable
fun SigningScreen(vm: SignumViewModel, goInstalls: () -> Unit) {
    val s = vm.state.collectAsState().value
    val c = signumColors
    val ctx = LocalContext.current
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        runCatching {
            ctx.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        val dest = File(ctx.cacheDir, "picked-${System.currentTimeMillis()}.ipa")
        ctx.contentResolver.openInputStream(uri)?.use { input ->
            dest.outputStream().use { input.copyTo(it) }
        }
        vm.setSignPath(dest.absolutePath)
    }
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
         PageHeader("Свой файл", "Выберите IPA и установите на iPhone")
        Panel {
            Text(
                "Android устанавливает IPA-файлы, уже подготовленные Apple ID или компьютером. " +
                    "Локальная подпись IPA на Android пока не выполняется.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
        }
        Panel {
            Text(
                if (s.auth.authenticated) "Apple ID: ${s.auth.email}" else "Войдите в Apple ID на вкладке «Аккаунт»",
                color = c.text,
                fontWeight = FontWeight.SemiBold,
                lineHeight = 20.sp,
            )
            Spacer(Modifier.height(8.dp))
            Text(
                if (s.signPath.isBlank()) "Файл не выбран" else File(s.signPath).name,
                color = c.muted,
                fontSize = 13.sp,
            )
            Spacer(Modifier.height(10.dp))
            SecondaryButton("Выбрать файл", onClick = { picker.launch(arrayOf("application/octet-stream", "*/*")) }, modifier = Modifier.fillMaxWidth())
            Spacer(Modifier.height(10.dp))
            SecondaryButton(
                "Установить IPA",
                onClick = {
                    vm.queueLocal(s.signPath, sign = false, install = true)
                    goInstalls()
                },
                enabled = s.signPath.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            )
            StatusLine(s.signMessage)
        }
    }
}
