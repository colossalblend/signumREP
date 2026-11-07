package pw.appbank.signum.data.api

import android.content.Context
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import org.json.JSONObject
import pw.appbank.signum.data.models.APP_BUILD
import pw.appbank.signum.data.models.APP_VERSION
import pw.appbank.signum.data.models.DEFAULT_API
import java.io.File

/**
 * Запись для поддержки.
 *
 * На ПК такая запись есть давно, и все находки последних дней — устаревший
 * движок на маке, двойной запрос на вход, истёкший вход — пришли именно из
 * присланных журналов. На Android её не было вовсе, и жалобы приходилось
 * разбирать вслепую.
 *
 * Пишем в файл, а не в память: человек включает запись, воспроизводит беду,
 * и приложение за это время вполне может быть выгружено системой.
 */
object DiagLog {
    private const val MAX_BYTES = 900_000L
    private const val PREF = "diag_recording"

    private const val PREF_AUTO = "diag_auto"

    @Volatile private var app: Context? = null
    @Volatile private var active = false
    @Volatile private var autoSession = false
    private val lock = Any()

    /** Разрешена ли самостоятельная запись. По умолчанию — да. */
    fun autoEnabled(): Boolean = prefs()?.getBoolean(PREF_AUTO, true) ?: true

    fun setAutoEnabled(on: Boolean) {
        prefs()?.edit()?.putBoolean(PREF_AUTO, on)?.apply()
        if (!on && autoSession) {
            // Человек снял разрешение посреди сессии — дописывать и отправлять
            // уже нечего.
            synchronized(lock) {
                active = false
                autoSession = false
                file()?.delete()
            }
        }
    }

    fun init(context: Context) {
        app = context.applicationContext
        val manual = prefs()?.getBoolean(PREF, false) ?: false
        // Самостоятельная запись: идёт всё время работы и уходит при закрытии.
        val auto = !manual && autoEnabled()
        autoSession = auto
        active = manual || auto
        if (auto) {
            // Прошлая запись уже отправлена; если приложение сняли из списка
            // задач, её остаток только запутает следующий отчёт.
            synchronized(lock) { file()?.delete() }
        }
        if (active) {
            note(
                "session",
                "start version=$APP_VERSION build=$APP_BUILD os=android " +
                    "запись=" + (if (manual) "вручную" else "автоматически"),
            )
        }
    }

    /**
     * Отправить накопленное при закрытии приложения.
     *
     * Молча: человек уже уходит, и окно с ошибкой ему показывать негде.
     */
    fun sendOnExit() {
        if (!active || !autoSession) return
        if (!autoEnabled()) {
            synchronized(lock) { file()?.delete() }
            return
        }
        runCatching { send() }
        synchronized(lock) { file()?.delete() }
    }

    fun isActive(): Boolean = active

    fun start() {
        synchronized(lock) {
            file()?.delete()
            active = true
            prefs()?.edit()?.putBoolean(PREF, true)?.apply()
        }
        note("session", "start version=$APP_VERSION build=$APP_BUILD os=android")
        note("session", "recording on")
    }

    fun stop() {
        synchronized(lock) {
            active = false
            prefs()?.edit()?.putBoolean(PREF, false)?.apply()
        }
    }

    fun note(tag: String, message: String) {
        if (!active) return
        append("${System.currentTimeMillis() / 1000} [$tag] ${redact(message)}")
    }

    /** Строка о запросе — в том же виде, что и на ПК. */
    fun http(method: String, url: String, code: Int, bodyLen: Long, preview: String?) {
        if (!active) return
        val tail = preview?.takeIf { it.isNotBlank() }?.let { " " + redact(it).take(160) }.orEmpty()
        append("${System.currentTimeMillis() / 1000} [http] << $method $url -> $code body(${bodyLen}b)$tail")
    }

    fun httpFail(method: String, url: String, error: String) {
        if (!active) return
        append("${System.currentTimeMillis() / 1000} [http] $method $url FAIL ${redact(error)}")
    }

    fun read(): String = runCatching { file()?.takeIf { it.isFile }?.readText() }.getOrNull().orEmpty()

    fun clear() {
        synchronized(lock) { file()?.delete() }
    }

    /** Отправить запись. Бросает с понятным текстом, если отправить нечего. */
    fun send(): Boolean {
        val log = read()
        if (log.isBlank()) error("Запись пуста. Включите запись, повторите проблему и отправьте снова.")
        val url = "${DEFAULT_API.trimEnd('/')}/logs.php".toHttpUrlOrNull() ?: return false
        // Тот же предохранитель, что у автоотчётов: отправляем только на свой
        // адрес, чтобы подменённая настройка не увела журнал на чужой сервер.
        if (url.host != Hx.apiHost()) return false
        val body = JSONObject()
            .put("kind", "support")
            // Отличаем самостоятельную запись от включённой руками: в сводке
            // на сервере они считаются отдельно.
            .put("source", if (autoSession) "auto-session" else "recording")
            .put("platform", "android")
            .put("version", APP_VERSION)
            .put("build", APP_BUILD)
            .put("log", log)
            .toString()
        val req = Request.Builder()
            .url(url)
            .post(body.toRequestBody("application/json; charset=utf-8".toMediaType()))
            .header("Accept", "application/json")
            .build()
        Http.api.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) error("Сервер не принял отчёт (${resp.code}).")
        }
        return true
    }

    private fun prefs() = app?.getSharedPreferences("signum", Context.MODE_PRIVATE)

    private fun file(): File? = app?.let { File(it.filesDir, "support-session.log") }

    private fun append(line: String) {
        val f = file() ?: return
        synchronized(lock) {
            runCatching {
                // Предел, чтобы длинная запись не съела память телефона и не
                // уткнулась в ограничение сервера на размер отчёта.
                if (f.length() > MAX_BYTES) {
                    val kept = f.readText().takeLast((MAX_BYTES / 2).toInt())
                    f.writeText("…начало записи отброшено…\n$kept")
                }
                f.appendText(line + "\n")
            }
        }
    }

    /**
     * Убрать из строки то, что не должно уезжать в чат поддержки.
     *
     * Набор тот же, что у автоотчётов и у сервера: почта, токены, куки,
     * шестизначные коды, имя пользователя в путях.
     */
    private fun redact(input: String): String {
        var text = input
        text = text.replace(Regex("(?i)[a-z0-9._%+\\-]+@[a-z0-9.\\-]+\\.[a-z]{2,}"), "[email]")
        text = text.replace(
            Regex("(?i)(bearer\\s+|token[=:]\\s*|passwordtoken[=:]\\s*|cookie[=:]\\s*|password[=:]\\s*)[^\\s,&]+"),
            "$1[hidden]",
        )
        text = text.replace(Regex("\\b\\d{6}\\b"), "[code]")
        // Имя владельца учётной записи приходит в ответе ipatool и в истории
        // покупок. Пока отчёты присылали вручную — полбеды; с автоотправкой
        // так нельзя, да и обещать «без личных данных» с именем внутри нельзя.
        text = text.replace(
            Regex("(?i)\"(name|firstName|lastName|fullName|dsid|phoneNumber)\"\\s*:\\s*\"[^\"]*\""),
            "\"$1\":\"[hidden]\"",
        )
        text = text.replace(Regex("(/(?:Users|home|data/user/0)/)[^/\\s]+"), "$1[user]")
        text = text.replace(Regex("\\b[0-9a-fA-F]{40}\\b"), "[id]")
        return text
    }
}
