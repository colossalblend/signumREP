package pw.appbank.signum.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import pw.appbank.signum.ui.theme.Brand300
import pw.appbank.signum.ui.theme.Brand500
import pw.appbank.signum.ui.theme.signumColors

@Composable
fun Panel(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    content: @Composable () -> Unit,
) {
    val c = signumColors
    val shape = RoundedCornerShape(18.dp)
    val elevation by animateDpAsState(if (onClick != null) 8.dp else 6.dp, label = "panel-elevation")
    Column(
        modifier
            .shadow(elevation, shape, ambientColor = Color.Black.copy(0.22f), spotColor = Color.Black.copy(0.18f))
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
    Column(Modifier.fillMaxWidth()) {
        Text(
            title,
            color = c.text,
            fontSize = 26.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = (-0.6).sp,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            subtitle,
            color = c.muted,
            fontSize = 13.sp,
            lineHeight = 18.sp,
        )
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
) {
    Button(
        onClick = onClick,
        enabled = enabled && !loading,
        modifier = modifier.heightIn(min = 44.dp),
        shape = RoundedCornerShape(12.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = Brand500,
            disabledContainerColor = Brand500.copy(0.4f),
        ),
    ) {
        if (loading) {
            CircularProgressIndicator(Modifier.size(16.dp), color = Color.White, strokeWidth = 2.dp)
            Spacer(Modifier.size(8.dp))
        }
        Text(
            text,
            fontWeight = FontWeight.SemiBold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
fun SecondaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val c = signumColors
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = 44.dp),
        shape = RoundedCornerShape(12.dp),
        colors = ButtonDefaults.outlinedButtonColors(contentColor = c.text),
    ) {
        Text(
            text,
            fontWeight = FontWeight.SemiBold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
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
            .background(c.background.copy(0.55f))
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

/** Заголовок раздела внутри панели — один стиль на все экраны. */
@Composable
fun SectionTitle(text: String) {
    Text(
        text,
        color = signumColors.text,
        fontSize = 15.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = (-0.2).sp,
    )
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
) {
    val c = signumColors
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.fillMaxWidth(),
        placeholder = { Text(placeholder, color = c.muted) },
        singleLine = singleLine,
        visualTransformation = visual,
        keyboardOptions = keyboard,
        trailingIcon = trailing,
        shape = RoundedCornerShape(12.dp),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = c.text,
            unfocusedTextColor = c.text,
            focusedBorderColor = Brand500.copy(0.6f),
            unfocusedBorderColor = c.border,
            cursorColor = Brand500,
            focusedContainerColor = c.background.copy(0.6f),
            unfocusedContainerColor = c.background.copy(0.6f),
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
                Text(text, color = Color.White.copy(0.75f), fontSize = 12.sp, lineHeight = 17.sp)
            }
            action?.invoke()
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
            .background(c.background.copy(0.6f))
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
        val shape = RoundedCornerShape(20.dp)
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(max = 540.dp)
                .clip(shape)
                .background(c.surfaceStrong)
                .border(1.dp, c.border, shape)
                .padding(16.dp),
        ) {
            Text(title, color = c.text, fontWeight = FontWeight.Bold, fontSize = 16.sp)
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
                items(filtered, key = { it.value }) { option ->
                    val isSelected = option.value == selected
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(if (isSelected) Brand500.copy(0.14f) else Color.Transparent)
                            .clickable { onPick(option.value) }
                            .padding(horizontal = 12.dp, vertical = 11.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                option.label,
                                color = if (isSelected) Brand300 else c.text,
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
                                tint = Brand300,
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

@Composable
fun StatusLine(text: String, error: Boolean = false) {
    if (text.isBlank()) return
    val c = signumColors
    Box(
        Modifier
            .padding(top = 8.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(if (error) Color(0x1FFF3B5B) else c.background.copy(alpha = 0.35f))
            .border(1.dp, if (error) Color(0x55FB7185) else c.border, RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Text(text, color = if (error) Color(0xFFFDA4AF) else c.muted, fontSize = 13.sp, lineHeight = 18.sp)
    }
}
