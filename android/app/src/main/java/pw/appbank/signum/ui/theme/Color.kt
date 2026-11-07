package pw.appbank.signum.ui.theme

import androidx.compose.ui.graphics.Color

// Грифельная основа — как на ПК (index.css, --app-bg и далее).
// Прежний почти-чёрный #070912 оставлен только как самый глубокий тон
// градиента, фоном он больше не служит.
val Space950 = Color(0xFF0F1420)
val Space900 = Color(0xFF131A27)
val Space850 = Color(0xFF161D2C)
val Space800 = Color(0xFF161D2C)

// Тот же набор, что на ПК (tailwind.config.js, раздел brand).
//
// Прежний #7C5CFF — самый растиражированный оттенок в сгенерированных
// интерфейсах, и на непрозрачных поверхностях он плохо читался. Десктоп
// перекрасили, Android остался на старом сиреневом — из-за этого приложения
// выглядели как разные продукты.
val Brand300 = Color(0xFFA5B4FC)
val Brand400 = Color(0xFF818CF8)
val Brand500 = Color(0xFF5B6CF0)
val Brand600 = Color(0xFF4453D8)

val TextDark = Color(0xFFEEF1F7)
val TextLight = Color(0xFF0F172A)
val Muted = Color(0xFF8B97AB)

// Непрозрачные поверхности вместо белого с прозрачностью 5 %: панель должна
// отделяться от фона сама, а не подсветкой под ней.
val SurfaceDark = Color(0xFF161D2C)
val SurfaceLight = Color(0xEBFFFFFF)
// Граница из шиферного оттенка с прозрачностью 16 % — её видно, в отличие от
// прежних 7 % белого.
val BorderDark = Color(0x2994A3B8)
val BorderLight = Color(0x1A0F172A)

val LightBg = Color(0xFFF4F6FB)
val LightHeader = Color(0xEBFFFFFF)
