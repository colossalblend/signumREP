package pw.appbank.signum.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color

data class SignumColors(
    val background: Color,
    val text: Color,
    val muted: Color,
    val border: Color,
    val surface: Color,
    val surfaceStrong: Color,
    val header: Color,
    val brand: Color,
    val brandSoft: Color,
    val isDark: Boolean,
) {
    val spaceBrush: Brush
        get() = if (isDark) {
            // Едва заметный переход вместо двух фиолетовых подсветок по углам:
            // именно они и читались как «свет ниоткуда».
            Brush.linearGradient(listOf(Space950, Space950, Color(0xFF0D121D)))
        } else {
            Brush.linearGradient(listOf(LightBg, Color(0xFFECEFF6)))
        }

    val brandBrush: Brush
        // Тот же переход, что в brand-gradient на ПК.
        get() = Brush.linearGradient(listOf(Color(0xFF5F70F2), Color(0xFF4B5CE2)))

    val blueBrush: Brush
        get() = Brush.linearGradient(listOf(Color(0xFF1686FF), Color(0xFF1556D7)))

    val amberBrush: Brush
        get() = Brush.linearGradient(listOf(Color(0xFFF5B72F), Color(0xFFDF7D14)))
}

val LocalSignumColors = staticCompositionLocalOf {
    SignumColors(
        background = Space950,
        text = TextDark,
        muted = Muted,
        border = BorderDark,
        surface = Space850,
        surfaceStrong = Space900,
        header = Color(0xEB0F1420),
        brand = Brand500,
        brandSoft = Brand300,
        isDark = true,
    )
}

private val DarkScheme = darkColorScheme(
    primary = Brand500,
    onPrimary = Color.White,
    background = Space950,
    onBackground = TextDark,
    surface = Space900,
    onSurface = TextDark,
    secondary = Brand400,
)

private val LightScheme = lightColorScheme(
    primary = Brand600,
    onPrimary = Color.White,
    background = LightBg,
    onBackground = TextLight,
    surface = Color.White,
    onSurface = TextLight,
    secondary = Brand500,
)

@Composable
fun SignumTheme(
    themePref: String = "dark",
    content: @Composable () -> Unit,
) {
    val systemDark = isSystemInDarkTheme()
    val dark = when (themePref) {
        "light" -> false
        "system" -> systemDark
        else -> true
    }
    val colors = if (dark) {
        SignumColors(
            background = Space950,
            text = TextDark,
            muted = Color(0xFF94A3B8),
            border = BorderDark,
            surface = Space850,
            surfaceStrong = Space900,
            header = Color(0xEB0F1420),
            brand = Brand500,
            brandSoft = Brand300,
            isDark = true,
        )
    } else {
        SignumColors(
            background = LightBg,
            text = TextLight,
            muted = Color(0xFF64748B),
            border = BorderLight,
            surface = Color(0xEBFFFFFF),
            surfaceStrong = Color(0xF5FFFFFF),
            header = LightHeader,
            brand = Brand600,
            brandSoft = Brand600,
            isDark = false,
        )
    }
    CompositionLocalProvider(LocalSignumColors provides colors) {
        MaterialTheme(
            colorScheme = if (dark) DarkScheme else LightScheme,
            typography = SignumTypography,
            content = content,
        )
    }
}

val signumColors: SignumColors
    @Composable get() = LocalSignumColors.current
