package pw.appbank.signum.ui.screens

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateDp
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.updateTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Apps
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private data class OnbSlide(
    val icon: ImageVector,
    val tag: String,
    val title: String,
    val text: String,
)

private val ONB_SLIDES = listOf(
    OnbSlide(
        Icons.Outlined.AutoAwesome,
        "ЗНАКОМСТВО",
        "Добро пожаловать в Signum",
        "Устанавливай приложения из App Store на iPhone и iPad прямо с телефона — быстро и без лишних шагов.",
    ),
    OnbSlide(
        Icons.Outlined.Apps,
        "ВКЛАДКА «КАТАЛОГ»",
        "Начни с Каталога",
        "Популярные приложения — банки, сервисы и другие — уже собраны в Каталоге. Выбери нужное и нажми «Установить».",
    ),
    OnbSlide(
        Icons.Outlined.Search,
        "ВКЛАДКА «ПОИСК»",
        "Нет в Каталоге? Найди в Поиске",
        "Если приложения нет в Каталоге — открой «Поиск», введи название, найди его в App Store и установи прямо оттуда.",
    ),
    OnbSlide(
        Icons.Outlined.History,
        "АККАУНТ → ИСТОРИЯ ПОКУПОК",
        "Не помнишь название?",
        "Загляни в «Аккаунт» → «История покупок». Все ранее скачанные приложения показаны с иконками и метками — выбирай и ставь.",
    ),
)

private val OnbViolet = Color(0xFF9A7BFF)
private val OnbIndigo = Color(0xFF6545E8)

@Composable
fun OnboardingScreen(reduceMotion: Boolean, onFinish: () -> Unit) {
    var index by remember { mutableIntStateOf(0) }
    val isLast = index == ONB_SLIDES.size - 1

    // Gentle icon float (disabled under reduce-motion).
    val floatY = if (reduceMotion) {
        0f
    } else {
        val infinite = rememberInfiniteTransition(label = "onb-float")
        val v by infinite.animateFloat(
            initialValue = -7f,
            targetValue = 7f,
            animationSpec = infiniteRepeatable(tween(1800, easing = LinearEasing), RepeatMode.Reverse),
            label = "onb-floatY",
        )
        v
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Brush.verticalGradient(listOf(Color(0xFF160F2E), Color(0xFF0A0717)))),
    ) {
        // Decorative glow blobs (radial gradients — no blur dependency).
        Box(
            Modifier
                .fillMaxSize()
                .background(
                    Brush.radialGradient(
                        colors = listOf(Color(0x558B6DFF), Color(0x00000000)),
                        center = Offset(160f, 220f),
                        radius = 820f,
                    ),
                ),
        )
        Box(
            Modifier
                .fillMaxSize()
                .background(
                    Brush.radialGradient(
                        colors = listOf(Color(0x40D84FD8), Color(0x00000000)),
                        center = Offset(1100f, 1900f),
                        radius = 900f,
                    ),
                ),
        )

        Text(
            "Пропустить",
            color = Color(0x99E2E8F0),
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .statusBarsPadding()
                .padding(top = 8.dp, end = 12.dp)
                .clip(RoundedCornerShape(10.dp))
                .clickable(onClick = onFinish)
                .padding(horizontal = 14.dp, vertical = 8.dp),
        )

        Column(
            Modifier
                .align(Alignment.CenterStart)
                .padding(horizontal = 28.dp)
                .padding(bottom = 40.dp),
            verticalArrangement = Arrangement.Center,
        ) {
            AnimatedContent(
                targetState = index,
                transitionSpec = {
                    (slideInHorizontally { full -> full / 3 } + fadeIn(tween(320))) togetherWith
                        (slideOutHorizontally { full -> -full / 3 } + fadeOut(tween(220)))
                },
                label = "onb-slide",
            ) { i ->
                val s = ONB_SLIDES[i]
                Column {
                    Box(
                        Modifier
                            .size(84.dp)
                            .graphicsLayer { translationY = floatY }
                            .clip(RoundedCornerShape(22.dp))
                            .background(Brush.linearGradient(listOf(OnbViolet, OnbIndigo))),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(s.icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(40.dp))
                    }
                    Spacer(Modifier.height(22.dp))
                    Text(s.tag, color = Color(0xFFB79BFF), fontSize = 11.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.2.sp)
                    Spacer(Modifier.height(10.dp))
                    Text(s.title, color = Color(0xFFF4F2FF), fontSize = 25.sp, fontWeight = FontWeight.ExtraBold, lineHeight = 29.sp)
                    Spacer(Modifier.height(12.dp))
                    Text(s.text, color = Color(0xDBD6D2F0), fontSize = 15.sp, lineHeight = 23.sp)
                }
            }
        }

        Row(
            Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(horizontal = 28.dp, vertical = 26.dp)
                .fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            val dots = updateTransition(index, label = "onb-dots")
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                ONB_SLIDES.indices.forEach { i ->
                    val w by dots.animateDp(label = "onb-dot") { cur -> if (cur == i) 26.dp else 8.dp }
                    val active = index == i
                    Box(
                        Modifier
                            .height(8.dp)
                            .width(w)
                            .clip(RoundedCornerShape(99.dp))
                            .background(
                                if (active) {
                                    Brush.horizontalGradient(listOf(OnbViolet, OnbIndigo))
                                } else {
                                    SolidColor(Color(if (i < index) 0x66936DFF else 0x30FFFFFF))
                                },
                            ),
                    )
                }
            }
            Button(
                onClick = { if (isLast) onFinish() else index += 1 },
                colors = ButtonDefaults.buttonColors(containerColor = OnbIndigo),
                shape = RoundedCornerShape(13.dp),
            ) {
                Text(if (isLast) "Начать" else "Далее", color = Color.White, fontWeight = FontWeight.Bold)
                Spacer(Modifier.width(4.dp))
                Icon(
                    if (isLast) Icons.Outlined.Check else Icons.Outlined.ChevronRight,
                    contentDescription = null,
                    tint = Color.White,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}
