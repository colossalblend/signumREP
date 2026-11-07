package pw.appbank.signum.data.settings

import android.content.Context
import android.content.ContentValues
import android.content.Intent
import android.content.SharedPreferences
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import androidx.core.content.edit
import androidx.documentfile.provider.DocumentFile
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKeys
import pw.appbank.signum.data.models.AppSettings
import pw.appbank.signum.data.models.AppleAccountInfo
import pw.appbank.signum.data.models.SessionState

class SettingsStore(context: Context) {
    private val app = context.applicationContext

    private val prefs: SharedPreferences =
        app.getSharedPreferences("signum_settings", Context.MODE_PRIVATE)

    private val secrets: SharedPreferences = runCatching {
        val master = MasterKeys.getOrCreate(MasterKeys.AES256_GCM_SPEC)
        EncryptedSharedPreferences.create(
            "signum_secrets",
            master,
            app,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }.getOrElse {
        app.getSharedPreferences("signum_secrets_fallback", Context.MODE_PRIVATE)
    }

    fun load(): AppSettings = AppSettings(
        theme = prefs.getString("theme", "dark") ?: "dark",
        autoUpdate = prefs.getBoolean("auto_update", true),
        liteDesign = prefs.getBoolean("lite_design", false),
        detailedErrors = prefs.getBoolean("detailed_errors", false),
        liteStaticLoaders = prefs.getBoolean("lite_static_loaders", true),
        liteReduceMotion = prefs.getBoolean("lite_reduce_motion", true),
        liteSimpleScrolling = prefs.getBoolean("lite_simple_scrolling", true),
        liteDisableBlur = prefs.getBoolean("lite_disable_blur", true),
        liteDisableShadows = prefs.getBoolean("lite_disable_shadows", true),
        liteFlatBackgrounds = prefs.getBoolean("lite_flat_backgrounds", true),
        selectedDeviceUdid = prefs.getString("selected_device_udid", null),
        downloadsDir = prefs.getString("downloads_dir", "") ?: "",
        exportsDir = prefs.getString("exports_dir", "") ?: "",
        favoriteAppIds = prefs.getStringSet("favorite_app_ids", emptySet())!!.toList().sorted(),
        ipatoolEngine = prefs.getString("ipatool_engine", "auto") ?: "auto",
    )

    fun save(settings: AppSettings) {
        prefs.edit {
            putString("theme", settings.theme)
            putBoolean("auto_update", settings.autoUpdate)
            putBoolean("lite_design", settings.liteDesign)
            putBoolean("detailed_errors", settings.detailedErrors)
            putBoolean("lite_static_loaders", settings.liteStaticLoaders)
            putBoolean("lite_reduce_motion", settings.liteReduceMotion)
            putBoolean("lite_simple_scrolling", settings.liteSimpleScrolling)
            putBoolean("lite_disable_blur", settings.liteDisableBlur)
            putBoolean("lite_disable_shadows", settings.liteDisableShadows)
            putBoolean("lite_flat_backgrounds", settings.liteFlatBackgrounds)
            putString("selected_device_udid", settings.selectedDeviceUdid)
            putString("downloads_dir", settings.downloadsDir)
            putString("exports_dir", settings.exportsDir)
            putStringSet("favorite_app_ids", settings.favoriteAppIds.toSet())
            putString("ipatool_engine", settings.ipatoolEngine)
        }
    }

    /**
     * Переключает закрепление приложения.
     *
     * Читаем-меняем-пишем прямо здесь, а не в UI: иначе одновременное
     * сохранение настроек с другого экрана затрёт список целиком.
     */
    fun toggleFavorite(appId: String): List<String> {
        val id = appId.trim()
        if (id.isEmpty()) return load().favoriteAppIds
        val set = prefs.getStringSet("favorite_app_ids", emptySet())!!.toMutableSet()
        if (!set.remove(id)) set.add(id)
        prefs.edit { putStringSet("favorite_app_ids", set) }
        return set.toList().sorted()
    }

    fun activeEmail(): String? = secrets.getString("active_email", null)?.trim()?.lowercase()
        ?.takeIf { it.isNotEmpty() }

    fun setActiveEmail(email: String?) {
        secrets.edit { putString("active_email", email?.trim()?.lowercase()) }
    }

    fun storePassword(email: String, password: String) {
        secrets.edit { putString("pw_${email.lowercase()}", password) }
    }

    fun loadPassword(email: String): String? =
        secrets.getString("pw_${email.lowercase()}", null)?.takeIf { it.isNotEmpty() }

    fun deletePassword(email: String) {
        secrets.edit { remove("pw_${email.lowercase()}") }
    }

    fun storeSessionBlob(email: String, json: String) {
        secrets.edit { putString("session_${email.lowercase()}", json) }
    }

    fun loadSessionBlob(email: String): String? =
        secrets.getString("session_${email.lowercase()}", null)

    fun deleteSessionBlob(email: String) {
        secrets.edit { remove("session_${email.lowercase()}") }
    }

    fun storeHistoryCookies(email: String, json: String) {
        secrets.edit { putString("rap_${email.lowercase()}", json) }
    }

    fun loadHistoryCookies(email: String): String? =
        secrets.getString("rap_${email.lowercase()}", null)

    fun deleteHistoryCookies(email: String) {
        secrets.edit { remove("rap_${email.lowercase()}") }
    }

    fun storeHistoryPending(email: String, json: String) {
        secrets.edit { putString("rap_pending_${email.lowercase()}", json) }
    }

    fun loadHistoryPending(email: String): String? =
        secrets.getString("rap_pending_${email.lowercase()}", null)?.takeIf { it.isNotBlank() }

    fun deleteHistoryPending(email: String) {
        secrets.edit { remove("rap_pending_${email.lowercase()}") }
    }

    // Снимок исходной страны магазина. Лежит в шифрованном хранилище и переживает
    // убийство процесса: без него прерванная смена страны оставит человека в чужой.
    fun storeRegionSnapshot(email: String, json: String) {
        secrets.edit { putString("region_${email.lowercase()}", json) }
    }

    fun loadRegionSnapshot(email: String): String? =
        secrets.getString("region_${email.lowercase()}", null)?.takeIf { it.isNotBlank() }

    fun deleteRegionSnapshot(email: String) {
        secrets.edit { remove("region_${email.lowercase()}") }
    }

    fun storeHistoryCache(email: String, json: String) {
        prefs.edit { putString("hist_cache_${email.lowercase()}", json) }
    }

    fun loadHistoryCache(email: String): String? =
        prefs.getString("hist_cache_${email.lowercase()}", null)

    fun accountAddedAt(email: String): Long =
        prefs.getLong("added_${email.lowercase()}", System.currentTimeMillis())

    fun markAccountAdded(email: String) {
        val key = "added_${email.lowercase()}"
        if (!prefs.contains(key)) prefs.edit { putLong(key, System.currentTimeMillis()) }
        val set = prefs.getStringSet("accounts", emptySet())!!.toMutableSet()
        set.add(email.lowercase())
        prefs.edit { putStringSet("accounts", set) }
    }

    fun removeAccount(email: String) {
        val key = email.lowercase()
        val set = prefs.getStringSet("accounts", emptySet())!!.toMutableSet()
        set.remove(key)
        prefs.edit {
            putStringSet("accounts", set)
            remove("added_$key")
        }
        secrets.edit {
            remove("pw_$key")
            // Сессия магазина пишется через storeSessionBlob("store_<почта>"),
            // то есть ключ "session_store_<почта>". Здесь удаляли
            // "session_<почта>" — не тот ключ, и вход в App Store переживал
            // удаление аккаунта.
            remove("session_store_$key")
            remove("rap_$key")
            remove("rap_pending_$key")
            remove("region_$key")
        }
        if (activeEmail() == key) setActiveEmail(set.firstOrNull())
    }

    fun knownEmails(): List<String> =
        prefs.getStringSet("accounts", emptySet())!!.toList().sorted()

    fun accountInfos(): List<AppleAccountInfo> {
        val active = activeEmail()
        return knownEmails().map { email ->
            val session = !loadSessionBlob(email).isNullOrBlank()
            AppleAccountInfo(
                email = email,
                active = email == active,
                sessionSaved = session,
                sessionState = when {
                    email == active && session -> SessionState.active
                    session -> SessionState.saved
                    else -> SessionState.not_saved
                },
                addedAt = accountAddedAt(email),
            )
        }
    }

    fun filesDir() = app.filesDir

    fun nativeLibDir(): String = app.applicationInfo.nativeLibraryDir
    fun cacheDir() = app.cacheDir
    // ───────────────────────────── Папка для скачанных IPA ─────────────────────
    //
    // Внутренние каталоги приложения пользователю недоступны, поэтому «скачать
    // файл» без выбранной папки заканчивался файлом, до которого не добраться.
    // Здесь лежит SAF-дерево, выбранное пользователем один раз.

    /**
     * Выбранная папка — или null, если её не выбирали либо доступ к ней
     * отозван (данные приложения чистили, папку удалили, карту вынули).
     *
     * Права проверяются здесь, а не при записи: иначе единственным признаком
     * потерянного доступа была бы упавшая загрузка в самом конце.
     */
    fun downloadsTree(): Uri? {
        val raw = prefs.getString("downloads_dir", "")?.trim().orEmpty()
        if (raw.isEmpty()) return null
        val uri = runCatching { Uri.parse(raw) }.getOrNull() ?: return null
        val granted = app.contentResolver.persistedUriPermissions.any {
            it.uri == uri && it.isWritePermission
        }
        return if (granted) uri else null
    }

    /** Запоминает папку и удерживает доступ к ней между запусками. */
    fun setDownloadsTree(uri: Uri?) {
        if (uri == null) {
            prefs.edit { putString("downloads_dir", "") }
            return
        }
        runCatching {
            app.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
        }
        prefs.edit { putString("downloads_dir", uri.toString()) }
    }

    /** Человекочитаемое имя папки для экрана настроек. */
    fun downloadsTreeLabel(): String {
        val uri = downloadsTree() ?: return ""
        val name = runCatching { DocumentFile.fromTreeUri(app, uri)?.name }.getOrNull()
        if (!name.isNullOrBlank()) return name
        // Последний сегмент вида "primary:Download" — на случай, когда
        // провайдер не отдаёт имя.
        return uri.lastPathSegment?.substringAfterLast(':')?.takeIf { it.isNotBlank() } ?: "Папка выбрана"
    }

    fun canExportToDownloads(): Boolean {
        if (!prefs.getString("downloads_dir", "").isNullOrBlank()) return downloadsTree() != null
        return Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q
    }

    /** Publish only a complete IPA. The caller removes temp after success. */
    fun exportToDownloads(file: java.io.File, displayName: String = file.name): String {
        val name = safeFileName(displayName)
        val tree = downloadsTree()
        if (tree == null && !prefs.getString("downloads_dir", "").isNullOrBlank()) {
            error("Нет доступа к выбранной папке. Выберите папку скачивания заново в настройках.")
        }
        if (tree != null) {
            val dir = DocumentFile.fromTreeUri(app, tree)
                ?: error("Не удалось открыть выбранную папку.")
            check(dir.canWrite()) { "Нет доступа к выбранной папке." }
            val uniqueName = if (dir.findFile(name) == null) name else
                "${name.removeSuffix(".ipa")}_${java.util.UUID.randomUUID()}.ipa"
            val doc = dir.createFile("application/octet-stream", uniqueName)
                ?: error("Не удалось создать файл в выбранной папке.")
            try {
                copyExport(file, doc.uri)
                return "${downloadsTreeLabel()}/${doc.name ?: uniqueName}"
            } catch (e: Exception) {
                runCatching { doc.delete() }
                throw e
            }
        }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            error("Выберите папку скачивания в настройках.")
        }
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name)
            put(MediaStore.MediaColumns.MIME_TYPE, "application/octet-stream")
            put(MediaStore.MediaColumns.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/Signum")
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val uri = app.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            ?: error("Не удалось создать файл в Downloads/Signum.")
        try {
            copyExport(file, uri)
            val complete = ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }
            check(app.contentResolver.update(uri, complete, null, null) > 0) {
                "Не удалось завершить сохранение IPA."
            }
            val actualName = runCatching {
                app.contentResolver.query(uri, arrayOf(MediaStore.MediaColumns.DISPLAY_NAME), null, null, null)
                    ?.use { if (it.moveToFirst()) it.getString(0) else null }
            }.getOrNull() ?: name
            return "Downloads/Signum/$actualName"
        } catch (e: Exception) {
            runCatching { app.contentResolver.delete(uri, null, null) }
            throw e
        }
    }

    private fun copyExport(file: java.io.File, uri: Uri) {
        val expected = file.length()
        app.contentResolver.openOutputStream(uri, "w")?.use { out ->
            val copied = file.inputStream().use { it.copyTo(out) }
            check(copied == expected) { "Файл приложения сохранился не полностью." }
            out.flush()
        } ?: error("Не удалось открыть файл для записи.")
    }

    /** Имя вида «СберМобайл 1.0.1.ipa» без символов, недопустимых в файловой системе. */
    fun safeFileName(raw: String): String {
        // Набор собран из символов, а не из регулярки: в char-class каждый из
        // них пришлось бы экранировать, и одна опечатка молча ломала бы имена.
        val forbidden = setOf('\\', '/', ':', '*', '?', '\"', '<', '>', '|')
        val cleaned = raw.trim()
            .map { if (it in forbidden || it.isISOControl()) ' ' else it }
            .joinToString("")
            .split(' ')
            .filter { it.isNotBlank() }
            .joinToString(" ")
            .take(120)
        val base = cleaned.ifBlank { "app" }
        return if (base.endsWith(".ipa", ignoreCase = true)) base else "$base.ipa"
    }

    fun ipaCacheDir() = java.io.File(app.filesDir, "ipa-cache").apply { mkdirs() }
    fun signedDir() = java.io.File(app.filesDir, "signed-ipas").apply { mkdirs() }
    fun pairingDir() = java.io.File(app.filesDir, "pairing").apply { mkdirs() }
    fun updatesDir() = java.io.File(app.filesDir, "updates").apply { mkdirs() }
    fun anisetteDir() = java.io.File(app.filesDir, "anisette").apply { mkdirs() }

    fun openAsset(path: String): java.io.InputStream = app.assets.open(path)

    fun onboardingSeen(): Boolean = prefs.getBoolean("onboarding_seen_v1", false)
    fun markOnboardingSeen() {
        prefs.edit().putBoolean("onboarding_seen_v1", true).apply()
    }
}
