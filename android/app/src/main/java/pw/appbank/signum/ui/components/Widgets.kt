package pw.appbank.signum.ui.components

import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowRight
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.WarningAmber
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import kotlinx.coroutines.launch
import pw.appbank.signum.ui.theme.Brand500
import pw.appbank.signum.ui.theme.signumColors

/**
 * Основная карточка.
 *
 * Тень только в тёмной теме и совсем мягкая: в светлой панель отделяется от
 * фона белым цветом и рамкой, а тень на белом превращала список в «грязные»
 * полосы. Нажимаемая карточка слегка проседает под пальцем — так видно, что
 * она отзывается, без отдельной кнопки.
 */
@Composable
fun Panel(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    content: @Composable () -> Unit,
) {
    val c = signumColors
    val shape = RoundedCornerShape(18.dp)
    Column(
        modifier
            .then(
                if (c.isDark) {
                    Modifier.shadow(4.dp, shape, ambientColor = Color.Black.copy(0.25f), spotColor = Color.Black.copy(0.2f))
                } else {
                    Modifier.shadow(1.dp, shape, ambientColor = Color(0x140F172A), spotColor = Color(0x140F172A))
                },
            )
            .animateContentSize()
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.border, shape)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(16.dp)
            .fillMaxWidth(),
    ) { content() }
}

@Composable
fun PageHeader(title: String, subtitle: String, actions: (@Composable RowScope.() -> Unit)? = null) {
    val c = signumColors
    Column(Modifier.fillMaxWidth().padding(top = 2.dp, bottom = 2.dp)) {
        Text(
            title,
            color = c.text,
            fontSize = 26.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = (-0.6).sp,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        if (subtitle.isNotBlank()) {
            Spacer(Modifier.height(4.dp))
            Text(
                subtitle,
                color = c.muted,
                fontSize = 13.sp,
                lineHeight = 18.sp,
            )
        }
        if (actions != null) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(top = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
                content = actions,
            )
        }
    }
}

@Composable
fun PrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
    icon: ImageVector? = null,
) {
    val active = enabled && !loading
    val c = signumColors
    val shape = RoundedCornerShape(12.dp)
    // Градиент вместо плоской заливки — как brand-gradient на ПК. Material
    // Button умеет только сплошной цвет, поэтому кнопка собрана вручную.
    Row(
        modifier
            .heightIn(min = 46.dp)
            .clip(shape)
            .background(c.brandBrush, shape, alpha = if (active) 1f else 0.45f)
            .clickable(enabled = active, role = Role.Button, onClick = onClick)
            .padding(horizontal = 18.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ButtonContent(text, icon, loading, Color.White.copy(alpha = if (active || loading) 1f else 0.8f))
    }
}

@Composable
private fun ButtonContent(text: String, icon: ImageVector?, loading: Boolean, tint: Color) {
    if (loading) {
        CircularProgressIndicator(Modifier.size(16.dp), color = tint, strokeWidth = 2.dp)
        Spacer(Modifier.size(8.dp))
    } else if (icon != null) {
        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(18.dp))
        Spacer(Modifier.size(8.dp))
    }
    Text(
        text,
        color = tint,
        fontWeight = FontWeight.SemiBold,
        fontSize = 14.sp,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        textAlign = TextAlign.Center,
    )
}

@Composable
fun SecondaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: ImageVector? = null,
    loading: Boolean = false,
) {
    val c = signumColors
    OutlinedButton(
        onClick = onClick,
        enabled = enabled && !loading,
        modifier = modifier.heightIn(min = 46.dp),
        shape = RoundedCornerShape(12.dp),
        border = androidx.compose.foundation.BorderStroke(1.dp, c.border),
        colors = ButtonDefaults.outlinedButtonColors(
            containerColor = c.inset.copy(alpha = if (c.isDark) 0.6f else 1f),
            contentColor = c.text,
            disabledContentColor = c.muted.copy(0.6f),
        ),
    ) {
        ButtonContent(text, icon, loading, if (enabled) c.text else c.muted)
    }
}

/** Опасное действие: выход, удаление, сброс. */
@Composable
fun DangerButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: ImageVector? = null,
    loading: Boolean = false,
) {
    val c = signumColors
    OutlinedButton(
        onClick = onClick,
        enabled = enabled && !loading,
        modifier = modifier.heightIn(min = 46.dp),
        shape = RoundedCornerShape(12.dp),
        border = androidx.compose.foundation.BorderStroke(1.dp, c.danger.copy(alpha = 0.35f)),
        colors = ButtonDefaults.outlinedButtonColors(
            containerColor = c.danger.copy(alpha = 0.08f),
            contentColor = c.danger,
            disabledContentColor = c.danger.copy(0.4f),
        ),
    ) {
        ButtonContent(text, icon, loading, c.danger)
    }
}

/**
 * Короткое текстовое действие в строке («Обновить», «Выбрать»).
 *
 * Раньше такие действия были голым Text с clickable: область нажатия — ровно
 * буквы, и попасть пальцем было трудно. Здесь минимум 40 dp по высоте.
 */
@Composable
fun TextAction(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    color: Color? = null,
) {
    val c = signumColors
    Box(
        modifier
            .heightIn(min = 40.dp)
            .clip(RoundedCornerShape(10.dp))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text,
            color = if (!enabled) c.muted else color ?: c.brandSoft,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            maxLines = 1,
        )
    }
}

/**
 * Выбор одного из нескольких коротких вариантов.
 *
 * Заменяет стопку кнопок во всю ширину: три варианта темы занимали треть
 * экрана и читались как три независимых действия, хотя это один переключатель.
 */
@Composable
fun SegmentedControl(
    options: List<Pair<String, String>>,
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = signumColors
    val shape = RoundedCornerShape(12.dp)
    Row(
        modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.inset)
            .border(1.dp, c.border, shape)
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        options.forEach { (id, label) ->
            val active = id == selected
            Box(
                Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(9.dp))
                    .background(if (active) Brand500 else Color.Transparent)
                    .clickable(enabled = !active) { onSelect(id) }
                    .padding(vertical = 9.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    label,
                    color = if (active) Color.White else c.muted,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/**
 * Фильтры-«таблетки» над списком: прокручивать не нужно, их всегда три-пять.
 * У каждого может быть счётчик — сколько записей под ним.
 */
@Composable
fun FilterPills(
    options: List<Triple<String, String, Int?>>,
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = signumColors
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        options.forEach { (id, label, count) ->
            val active = id == selected
            val shape = RoundedCornerShape(50)
            Row(
                Modifier
                    .clip(shape)
                    .background(if (active) c.brand.copy(alpha = 0.16f) else c.inset)
                    .border(1.dp, if (active) c.brand.copy(alpha = 0.45f) else c.border, shape)
                    .clickable { onSelect(id) }
                    .padding(horizontal = 12.dp, vertical = 7.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    label,
                    color = if (active) c.brandSoft else c.muted,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                )
                if (count != null && count > 0) {
                    Spacer(Modifier.width(5.dp))
                    Text(
                        count.toString(),
                        color = if (active) c.brandSoft else c.muted,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
    }
}

/** Заголовок раздела внутри панели — один стиль на все экраны. */
@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier, trailing: (@Composable RowScope.() -> Unit)? = null) {
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            text,
            color = signumColors.text,
            fontSize = 15.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = (-0.2).sp,
            modifier = Modifier.weight(1f),
        )
        trailing?.invoke(this)
    }
}

/**
 * Подпись над группой карточек в ленте (вне панели): «ИСТОРИЯ», «ОБНОВЛЕНИЯ».
 */
@Composable
fun SectionHeader(text: String, modifier: Modifier = Modifier, trailing: (@Composable RowScope.() -> Unit)? = null) {
    val c = signumColors
    Row(
        modifier
            .fillMaxWidth()
            .padding(top = 6.dp, start = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text.uppercase(),
            color = c.muted,
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 0.9.sp,
            modifier = Modifier.weight(1f),
        )
        trailing?.invoke(this)
    }
}

@Composable
fun SignumField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    singleLine: Boolean = true,
    visual: VisualTransformation = VisualTransformation.None,
    keyboard: KeyboardOptions = KeyboardOptions.Default,
    trailing: @Composable (() -> Unit)? = null,
    leading: @Composable (() -> Unit)? = null,
    label: String? = null,
    enabled: Boolean = true,
) {
    val c = signumColors
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.fillMaxWidth(),
        placeholder = { Text(placeholder, color = c.muted) },
        label = label?.let { { Text(it) } },
        singleLine = singleLine,
        enabled = enabled,
        visualTransformation = visual,
        keyboardOptions = keyboard,
        trailingIcon = trailing,
        leadingIcon = leading,
        shape = RoundedCornerShape(12.dp),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = c.text,
            unfocusedTextColor = c.text,
            disabledTextColor = c.muted,
            focusedBorderColor = c.brand.copy(0.7f),
            unfocusedBorderColor = c.border,
            disabledBorderColor = c.border,
            cursorColor = c.brand,
            focusedLabelColor = c.brandSoft,
            unfocusedLabelColor = c.muted,
            focusedLeadingIconColor = c.muted,
            unfocusedLeadingIconColor = c.muted,
            focusedContainerColor = c.inset,
            unfocusedContainerColor = c.inset,
            disabledContainerColor = c.inset,
        ),
    )
}

@Composable
fun PromoBanner(title: String, text: String, amber: Boolean = false, action: @Composable (() -> Unit)? = null) {
    val brush = if (amber) {
        Brush.linearGradient(listOf(Color(0xFFF5B72F), Color(0xFFDF7D14)))
    } else {
        Brush.linearGradient(listOf(Color(0xFF1686FF), Color(0xFF1556D7)))
    }
    Box(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(brush)
            .padding(18.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f, fill = false).fillMaxWidth()) {
                Text(title, color = Color.White, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                Spacer(Modifier.height(4.dp))
                Text(text, color = Color.White.copy(0.82f), fontSize = 12.sp, lineHeight = 17.sp)
            }
            action?.invoke()
        }
    }
}

enum class NoticeTone { Info, Warning, Danger, Success }

/**
 * Плашка-предупреждение внутри ленты: заголовок, текст и необязательное
 * действие. Одна для всех экранов, чтобы жёлтое и красное выглядели одинаково.
 */
@Composable
fun Notice(
    title: String,
    text: String = "",
    tone: NoticeTone = NoticeTone.Info,
    modifier: Modifier = Modifier,
    action: (@Composable () -> Unit)? = null,
) {
    val c = signumColors
    val accent = when (tone) {
        NoticeTone.Info -> c.brandSoft
        NoticeTone.Warning -> c.warning
        NoticeTone.Danger -> c.danger
        NoticeTone.Success -> c.success
    }
    val icon = when (tone) {
        NoticeTone.Info, NoticeTone.Success -> Icons.Outlined.Info
        NoticeTone.Warning -> Icons.Outlined.WarningAmber
        NoticeTone.Danger -> Icons.Outlined.ErrorOutline
    }
    val shape = RoundedCornerShape(16.dp)
    Row(
        modifier
            .fillMaxWidth()
            .clip(shape)
            .background(accent.copy(alpha = 0.09f))
            .border(1.dp, accent.copy(alpha = 0.3f), shape)
            .padding(14.dp),
    ) {
        Icon(icon, contentDescription = null, tint = accent, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = c.text, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, lineHeight = 19.sp)
            if (text.isNotBlank()) {
                Spacer(Modifier.height(3.dp))
                Text(text, color = c.muted, fontSize = 12.sp, lineHeight = 17.sp)
            }
            if (action != null) {
                Spacer(Modifier.height(10.dp))
                action()
            }
        }
    }
}

/**
 * Пустое состояние: значок в круге, заголовок, пояснение и действие.
 * Раньше пустые списки показывали голую строку текста в карточке.
 */
@Composable
fun EmptyState(
    icon: ImageVector,
    title: String,
    text: String,
    modifier: Modifier = Modifier,
    action: (@Composable () -> Unit)? = null,
) {
    val c = signumColors
    Panel(modifier) {
        Column(
            Modifier.fillMaxWidth().padding(vertical = 10.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                Modifier
                    .size(56.dp)
                    .clip(CircleShape)
                    .background(c.brand.copy(alpha = 0.12f)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(icon, contentDescription = null, tint = c.brandSoft, modifier = Modifier.size(26.dp))
            }
            Spacer(Modifier.height(12.dp))
            Text(title, color = c.text, fontWeight = FontWeight.Bold, fontSize = 16.sp, textAlign = TextAlign.Center)
            Spacer(Modifier.height(6.dp))
            Text(
                text,
                color = c.muted,
                fontSize = 13.sp,
                lineHeight = 18.sp,
                textAlign = TextAlign.Center,
            )
            if (action != null) {
                Spacer(Modifier.height(14.dp))
                action()
            }
        }
    }
}

/**
 * Серый прямоугольник-заглушка, мягко «дышит», пока грузятся данные.
 * При reduceMotion не анимируется — просто стоит ровным цветом.
 */
@Composable
fun SkeletonBlock(
    modifier: Modifier = Modifier,
    height: Dp = 14.dp,
    corner: Dp = 7.dp,
    reduceMotion: Boolean = false,
) {
    val c = signumColors
    val alpha = if (reduceMotion) {
        0.8f
    } else {
        val transition = rememberInfiniteTransition(label = "skeleton")
        val a by transition.animateFloat(
            initialValue = 0.45f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(850, easing = LinearEasing), RepeatMode.Reverse),
            label = "skeleton-alpha",
        )
        a
    }
    Box(
        modifier
            .height(height)
            .graphicsLayer { this.alpha = alpha }
            .clip(RoundedCornerShape(corner))
            .background(c.track),
    )
}

/** Карточка-заглушка списка: значок и две строки. */
@Composable
fun SkeletonRow(reduceMotion: Boolean = false) {
    Panel {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SkeletonBlock(Modifier.size(44.dp), height = 44.dp, corner = 12.dp, reduceMotion = reduceMotion)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                SkeletonBlock(Modifier.fillMaxWidth(0.6f), reduceMotion = reduceMotion)
                Spacer(Modifier.height(8.dp))
                SkeletonBlock(Modifier.fillMaxWidth(0.35f), height = 10.dp, reduceMotion = reduceMotion)
            }
        }
    }
}

/** Небольшая цветная метка статуса: «Установлено», «Ошибка», «Сгружено». */
@Composable
fun StatusChip(text: String, color: Color, modifier: Modifier = Modifier) {
    Box(
        modifier
            .clip(RoundedCornerShape(50))
            .background(color.copy(alpha = 0.14f))
            .padding(horizontal = 9.dp, vertical = 3.dp),
    ) {
        Text(text, color = color, fontSize = 11.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
    }
}

/** Плавная полоса прогресса со скруглёнными концами. */
@Composable
fun ProgressBar(progress: Float, modifier: Modifier = Modifier, color: Color? = null) {
    val c = signumColors
    val animated by animateFloatAsState(progress.coerceIn(0f, 1f), tween(300), label = "progress")
    Box(
        modifier
            .fillMaxWidth()
            .height(6.dp)
            .clip(RoundedCornerShape(3.dp))
            .background(c.track),
    ) {
        Box(
            Modifier
                .fillMaxWidth(animated)
                .fillMaxHeight()
                .clip(RoundedCornerShape(3.dp))
                .background(color?.let { Brush.linearGradient(listOf(it, it)) } ?: c.brandBrush),
        )
    }
}

/** Строка «название — значение» для сведений об устройстве и файле. */
@Composable
fun InfoRow(label: String, value: String, modifier: Modifier = Modifier) {
    val c = signumColors
    Row(modifier.fillMaxWidth().padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
        Spacer(Modifier.width(12.dp))
        Text(
            value,
            color = c.text,
            fontSize = 13.sp,
            fontWeight = FontWeight.Medium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.End,
            modifier = Modifier.weight(1.4f, fill = false),
        )
    }
}

/** Переключатель с подписью и необязательным пояснением. */
@Composable
fun ToggleRow(
    label: String,
    checked: Boolean,
    hint: String? = null,
    enabled: Boolean = true,
    onChange: (Boolean) -> Unit,
) {
    val c = signumColors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .clickable(enabled = enabled) { onChange(!checked) }
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(label, color = if (enabled) c.text else c.muted, fontSize = 14.sp)
            if (!hint.isNullOrBlank()) {
                Spacer(Modifier.height(2.dp))
                Text(hint, color = c.muted, fontSize = 12.sp, lineHeight = 16.sp)
            }
        }
        Spacer(Modifier.width(12.dp))
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            enabled = enabled,
            colors = SwitchDefaults.colors(
                checkedTrackColor = Brand500,
                checkedThumbColor = Color.White,
                uncheckedTrackColor = c.track,
                uncheckedBorderColor = c.border,
                uncheckedThumbColor = c.muted,
            ),
        )
    }
}

/** Тонкая линия между строками внутри панели. */
@Composable
fun RowDivider(modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .height(1.dp)
            .background(signumColors.divider),
    )
}

/** Квадратная галочка выбора — видна в обеих темах. */
@Composable
fun CheckBox(checked: Boolean, modifier: Modifier = Modifier) {
    val c = signumColors
    val shape = RoundedCornerShape(6.dp)
    Box(
        modifier
            .size(22.dp)
            .clip(shape)
            .background(if (checked) c.brand else Color.Transparent)
            .border(1.5.dp, if (checked) c.brand else c.muted.copy(alpha = 0.6f), shape),
        contentAlignment = Alignment.Center,
    ) {
        if (checked) {
            Icon(Icons.Filled.Check, contentDescription = null, tint = Color.White, modifier = Modifier.size(15.dp))
        }
    }
}

data class PickerOption(val value: String, val label: String, val hint: String? = null)

/** Поле-триггер: выглядит как SignumField, но открывает PickerDialog. */
@Composable
fun SelectField(
    value: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "Выберите",
    enabled: Boolean = true,
    hint: String? = null,
) {
    val c = signumColors
    val shape = RoundedCornerShape(12.dp)
    Row(
        modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.inset)
            .border(1.dp, c.border, shape)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            value.ifBlank { placeholder },
            color = if (value.isBlank() || !enabled) c.muted else c.text,
            fontSize = 14.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (!hint.isNullOrBlank()) {
            Text(hint, color = c.muted, fontSize = 12.sp, modifier = Modifier.padding(start = 8.dp))
        }
        Icon(
            Icons.Filled.ExpandMore,
            contentDescription = null,
            tint = c.muted,
            modifier = Modifier.padding(start = 8.dp).size(18.dp),
        )
    }
}

@Composable
fun PickerDialog(
    title: String,
    options: List<PickerOption>,
    selected: String?,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
    searchPlaceholder: String = "Поиск…",
) {
    val c = signumColors
    var query by remember { mutableStateOf("") }
    val filtered = remember(query, options) {
        val q = query.trim()
        if (q.isBlank()) options
        else options.filter {
            it.label.contains(q, ignoreCase = true) || it.hint?.contains(q, ignoreCase = true) == true
        }
    }
    Dialog(onDismissRequest = onDismiss) {
        val shape = RoundedCornerShape(24.dp)
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(max = 540.dp)
                .clip(shape)
                .background(c.surfaceStrong)
                .border(1.dp, c.border, shape)
                .padding(18.dp),
        ) {
            Text(title, color = c.text, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            if (options.size > 8) {
                Spacer(Modifier.height(10.dp))
                SignumField(query, { query = it }, placeholder = searchPlaceholder)
            }
            Spacer(Modifier.height(10.dp))
            LazyColumn(
                Modifier.weight(1f, fill = false),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                if (filtered.isEmpty()) {
                    item {
                        Text(
                            "Ничего не найдено",
                            color = c.muted,
                            fontSize = 13.sp,
                            modifier = Modifier.padding(vertical = 12.dp),
                        )
                    }
                }
                items(filtered.distinctBy { it.value }, key = { it.value }) { option ->
                    val isSelected = option.value == selected
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(if (isSelected) c.brand.copy(0.14f) else Color.Transparent)
                            .clickable { onPick(option.value) }
                            .padding(horizontal = 12.dp, vertical = 11.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                option.label,
                                color = if (isSelected) c.brandSoft else c.text,
                                fontSize = 14.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            if (!option.hint.isNullOrBlank()) {
                                Text(option.hint, color = c.muted, fontSize = 11.sp)
                            }
                        }
                        if (isSelected) {
                            Icon(
                                Icons.Filled.Check,
                                contentDescription = null,
                                tint = c.brandSoft,
                                modifier = Modifier.size(18.dp),
                            )
                        }
                    }
                }
            }
            Spacer(Modifier.height(10.dp))
            SecondaryButton("Закрыть", onClick = onDismiss, modifier = Modifier.fillMaxWidth())
        }
    }
}

/**
 * Простое окно подтверждения для необратимых действий.
 */
@Composable
fun ConfirmDialog(
    title: String,
    text: String,
    confirm: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    danger: Boolean = true,
) {
    val c = signumColors
    Dialog(onDismissRequest = onDismiss) {
        val shape = RoundedCornerShape(24.dp)
        Column(
            Modifier
                .fillMaxWidth()
                .clip(shape)
                .background(c.surfaceStrong)
                .border(1.dp, c.border, shape)
                .padding(20.dp),
        ) {
            Text(title, color = c.text, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            Spacer(Modifier.height(8.dp))
            Text(text, color = c.muted, fontSize = 13.sp, lineHeight = 18.sp)
            Spacer(Modifier.height(18.dp))
            if (danger) {
                DangerButton(confirm, onClick = onConfirm, modifier = Modifier.fillMaxWidth())
            } else {
                PrimaryButton(confirm, onClick = onConfirm, modifier = Modifier.fillMaxWidth())
            }
            Spacer(Modifier.height(8.dp))
            SecondaryButton("Отмена", onClick = onDismiss, modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
fun StatusLine(text: String, error: Boolean = false) {
    if (text.isBlank()) return
    val c = signumColors
    val shape = RoundedCornerShape(12.dp)
    Box(
        Modifier
            .padding(top = 8.dp)
            .fillMaxWidth()
            .clip(shape)
            .background(if (error) c.danger.copy(alpha = 0.1f) else c.inset)
            .border(1.dp, if (error) c.danger.copy(alpha = 0.35f) else c.border, shape)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Text(text, color = if (error) c.danger else c.muted, fontSize = 13.sp, lineHeight = 18.sp)
    }
}

/**
 * Копирование текста в буфер обмена.
 *
 * Новый LocalClipboard работает через suspend-функцию, поэтому возвращаем
 * обычную лямбду, которая сама запускает запись в области экрана.
 */
@Composable
fun rememberCopyText(): (String) -> Unit {
    val clipboard = androidx.compose.ui.platform.LocalClipboard.current
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    return remember(clipboard, scope) {
        { text: String ->
            scope.launch {
                runCatching {
                    clipboard.setClipEntry(
                        androidx.compose.ui.platform.ClipEntry(
                            android.content.ClipData.newPlainText("Signum", text),
                        ),
                    )
                }
            }
            Unit
        }
    }
}

/**
 * Строка-переход внутри панели: значок, заголовок, пояснение и стрелка.
 * Для «О приложении», ссылок и прочего, что открывает другой экран.
 */
@Composable
fun NavRow(
    icon: ImageVector,
    title: String,
    subtitle: String? = null,
    trailingIcon: ImageVector = Icons.AutoMirrored.Outlined.KeyboardArrowRight,
    onClick: () -> Unit,
) {
    val c = signumColors
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .clip(RoundedCornerShape(12.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(38.dp)
                .clip(RoundedCornerShape(11.dp))
                .background(c.brand.copy(alpha = 0.12f)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, tint = c.brandSoft, modifier = Modifier.size(20.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = c.text, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (!subtitle.isNullOrBlank()) {
                Spacer(Modifier.height(2.dp))
                Text(subtitle, color = c.muted, fontSize = 12.sp, lineHeight = 16.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.width(8.dp))
        Icon(trailingIcon, contentDescription = null, tint = c.muted, modifier = Modifier.size(18.dp))
    }
}

/**
 * Поле настроек, которое держит текст у себя.
 *
 * Если отдавать полю значение прямо из StateFlow настроек, оно приходит
 * с опозданием на кадр: при быстром наборе буквы терялись, а курсор прыгал
 * в конец. Здесь набор идёт в локальное состояние, а наружу уходит копия.
 */
@Composable
fun SettingsTextField(
    value: String,
    onCommit: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    visual: VisualTransformation = VisualTransformation.None,
    keyboard: KeyboardOptions = KeyboardOptions.Default,
    filter: (String) -> String = { it },
) {
    var text by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf(value) }
    // Что мы сами недавно отдали наружу. Запоздавшее эхо любого из этих
    // значений пропускаем, иначе оно стирало бы только что набранные буквы.
    val recent = remember { ArrayDeque<String>() }
    // Значение поменялось снаружи (сброс, другая учётка) — подхватываем.
    androidx.compose.runtime.LaunchedEffect(value) {
        if (value != text && value !in recent) {
            text = value
            recent.clear()
        }
    }
    SignumField(
        value = text,
        onValueChange = { raw ->
            val next = filter(raw)
            text = next
            recent.addLast(next)
            while (recent.size > 32) recent.removeFirst()
            onCommit(next)
        },
        modifier = modifier,
        placeholder = placeholder,
        label = label,
        visual = visual,
        keyboard = keyboard,
    )
}
