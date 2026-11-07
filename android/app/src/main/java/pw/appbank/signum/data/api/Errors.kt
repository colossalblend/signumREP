package pw.appbank.signum.data.api

object Errors {
    var detailed: Boolean = false

    fun localize(raw: String): String {
        val message = raw.trim().ifEmpty { "Не удалось выполнить операцию" }
        val shown = localizeShown(message)
        if (!message.startsWith("SIGNUM_2FA_REQUIRED:")) {
            AutoLog.report("ui", shown, raw)
        }
        return shown
    }

    private fun localizeShown(message: String): String {
        if (detailed) return message
        val lower = message.lowercase()
        if (message.startsWith("SIGNUM_2FA_REQUIRED:")) return message
        if (message.startsWith("SIGNUM_LOGIN_OPAQUE:")) {
            return message.removePrefix("SIGNUM_LOGIN_OPAQUE:").trim()
        }
        return when {
            "anisette" in lower || "x-apple-i-md" in lower ->
                "Не удалось подготовить вход в Apple ID. Проверьте интернет и повторите."
            "certificate pinning" in lower || "sslpeerunverified" in lower || "pin mismatch" in lower ->
                "Не удалось подключиться к каталогу (проверка сертификата). Отключите прокси/VPN и повторите."
            "unacceptable certificate" in lower || "apple root ca" in lower || "certpath" in lower ||
                "trust anchor" in lower || "sslhandshake" in lower ->
                "Не удалось установить защищённое соединение с Apple. Проверьте интернет и повторите."
            "неверный apple id" in lower || "incorrect password" in lower || "invalid password" in lower ||
                "correct password" in lower || "wrong password" in lower ->
                "Apple не принял пароль. Проверьте раскладку и повторите. Если пароль верный — подождите минуту и войдите снова."
            "session expired" in lower || "not authenticated" in lower || "token is expired" in lower ||
                "password token" in lower ->
                "Вход в Apple ID устарел. Откройте «Аккаунт», выйдите и войдите снова."
            "unknown error" in lower || "sign in to the itunes" in lower || "itunes store" in lower ->
                "App Store просит повторный вход. Откройте «Аккаунт», выйдите и войдите снова, затем повторите установку."
            "terms and conditions" in lower || "media services" in lower || "мультимедийн" in lower ||
                "положение и условия" in lower || ("условия" in lower && "измен" in lower) ->
                "Apple просит подтвердить условия. Подождите минуту и повторите установку."
            "no device" in lower || "device not found" in lower ->
                "iPhone не найден. Подключите устройство кабелем, разблокируйте его и нажмите «Доверять»."
            "afc" in lower || "installation_proxy" in lower || "publicstaging" in lower ->
                "Не удалось передать файл на iPhone. Проверьте кабель и повторите."
            "нет порта" in lower || "no matching session" in lower || "usbmux" in lower || "mux" in lower ->
                "Не удалось подключиться к iPhone. Переподключите кабель и повторите."
            "tls" in lower || "clienthello" in lower || "startsession" in lower ->
                "Не удалось установить связь с iPhone. Разблокируйте его, нажмите «Доверять» и повторите."
            "trust" in lower || "pairing" in lower || "lockdown" in lower ->
                "Нет доверия. Разблокируйте iPhone, нажмите «Доверять» и повторите."
            "info.plist" in lower || "cfbundle" in lower || "mach-o" in lower || "codesign" in lower ->
                "Не удалось прочитать файл приложения."
            "srp" in lower || "не вернул salt" in lower || "не вернул b" in lower ||
                "не вернул c" in lower || "не вернул m2" in lower || "не вернул spd" in lower ->
                "Apple не подтвердил вход. Проверьте пароль и повторите."
            "идентификатора" in lower || "нет url" in lower || "dspersonid" in lower || "нет токена" in lower ->
                "Не удалось начать установку. Выйдите и войдите в Apple ID, затем выберите версию снова."
            "property list" in lower || "неодиданн" in lower || "неожиданный ответ" in lower ->
                "Apple вернул неожиданный ответ. Повторите вход."
            "timeout" in lower || "timed out" in lower ->
                "Операция заняла слишком много времени. Проверьте подключение и повторите."
            "connection reset" in lower || "connection abort" in lower || "broken pipe" in lower ->
                "Скачивание оборвалось. Проверьте интернет и повторите установку."
            "unable to resolve host" in lower || "failed to connect" in lower || "network" in lower ->
                "Не удалось подключиться к сервису. Проверьте интернет и повторите попытку."
            message.any { it in 'А'..'я' || it == 'ё' || it == 'Ё' } -> message
            else -> "Не удалось выполнить операцию. Повторите попытку."
        }
    }
}
