package pw.appbank.signum.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

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
    // Смысловые цвета. Раньше каждый экран брал свой оттенок красного и
    // жёлтого, и в светлой теме часть надписей просто терялась на белом.
    val success: Color = if (isDark) SuccessDark else SuccessLight,
    val warning: Color = if (isDark) WarningDark else WarningLight,
    val danger: Color = if (isDark) DangerDark else DangerLight,
    /** Подложка полей и переключателей внутри панели. */
    val inset: Color = if (isDark) Color(0xFF0F1521) else Color(0xFFF1F4FA),
    /** Тонкий разделитель строк внутри панели. */
    val divider: Color = if (isDark) Color(0x1F94A3B8) else Color(0x140F172A),
    /** Цвет «пустой» дорожки прогресса и скелетонов. */
    val track: Color = if (isDark) Color(0xFF1E2738) else Color(0xFFE3E8F1),
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

private val SuccessDark = Color(0xFF4ADE80)
private val SuccessLight = Color(0xFF15803D)
private val WarningDark = Color(0xFFFBBF24)
private val WarningLight = Color(0xFFB45309)
private val DangerDark = Color(0xFFF87171)
private val DangerLight = Color(0xFFDC2626)

private val DarkColors = SignumColors(
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

private val LightColors = SignumColors(
    background = LightBg,
    text = TextLight,
    muted = Color(0xFF64748B),
    border = BorderLight,
    surface = Color(0xFFFFFFFF),
    surfaceStrong = Color(0xFFFFFFFF),
    header = LightHeader,
    brand = Brand600,
    // Brand300 на белом не читается: в светлой теме «мягкий» акцент — это
    // тот же насыщенный индиго.
    brandSoft = Brand600,
    isDark = false,
)

val LocalSignumColors = staticCompositionLocalOf { DarkColors }

// Полные схемы: Switch, FilterChip, NavigationBar, поля и диалоги Material
// берут цвета отсюда. С неполной схемой они подставляли сиреневые цвета по
// умолчанию, и стандартные элементы выглядели чужими рядом со своими.
private val DarkScheme = darkColorScheme(
    primary = Brand500,
    onPrimary = Color.White,
    primaryContainer = Color(0xFF262F5C),
    onPrimaryContainer = Brand300,
    secondary = Brand400,
    onSecondary = Color.White,
    secondaryContainer = Color(0xFF232B45),
    onSecondaryContainer = Brand300,
    tertiary = Color(0xFF38BDF8),
    background = Space950,
    onBackground = TextDark,
    surface = Space900,
    onSurface = TextDark,
    surfaceVariant = Color(0xFF1C2435),
    onSurfaceVariant = Color(0xFF94A3B8),
    surfaceContainerLowest = Space950,
    surfaceContainerLow = Space900,
    surfaceContainer = Space850,
    surfaceContainerHigh = Color(0xFF1B2333),
    surfaceContainerHighest = Color(0xFF212A3C),
    outline = Color(0xFF3A465C),
    outlineVariant = Color(0xFF263044),
    error = DangerDark,
    onError = Color(0xFF450A0A),
    errorContainer = Color(0x33F87171),
    onErrorContainer = Color(0xFFFECACA),
    inverseSurface = TextDark,
    inverseOnSurface = Space950,
    scrim = Color(0xCC000000),
)

private val LightScheme = lightColorScheme(
    primary = Brand600,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE0E4FF),
    onPrimaryContainer = Color(0xFF1E2A8A),
    secondary = Brand500,
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE6E9FB),
    onSecondaryContainer = Color(0xFF1E2A8A),
    tertiary = Color(0xFF0284C7),
    background = LightBg,
    onBackground = TextLight,
    surface = Color.White,
    onSurface = TextLight,
    surfaceVariant = Color(0xFFEDF0F7),
    onSurfaceVariant = Color(0xFF64748B),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color(0xFFF8FAFD),
    surfaceContainer = Color(0xFFF1F4FA),
    surfaceContainerHigh = Color(0xFFEBEFF6),
    surfaceContainerHighest = Color(0xFFE4E9F2),
    outline = Color(0xFFCBD3E1),
    outlineVariant = Color(0xFFE2E7F0),
    error = DangerLight,
    onError = Color.White,
    errorContainer = Color(0xFFFEE2E2),
    onErrorContainer = Color(0xFF7F1D1D),
    inverseSurface = Color(0xFF1E293B),
    inverseOnSurface = Color.White,
    scrim = Color(0x99000000),
)

/** Скругления: мелкие элементы — 10, кнопки и поля — 12, панели — 18, листы — 24. */
val SignumShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(18.dp),
    extraLarge = RoundedCornerShape(24.dp),
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
    CompositionLocalProvider(LocalSignumColors provides if (dark) DarkColors else LightColors) {
        MaterialTheme(
            colorScheme = if (dark) DarkScheme else LightScheme,
            typography = SignumTypography,
            shapes = SignumShapes,
            content = content,
        )
    }
}

val signumColors: SignumColors
    @Composable get() = LocalSignumColors.current
