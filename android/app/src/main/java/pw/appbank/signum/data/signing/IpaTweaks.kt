package pw.appbank.signum.data.signing

import com.dd.plist.NSArray
import com.dd.plist.NSDictionary
import com.dd.plist.NSString
import pw.appbank.signum.data.apple.Plist
import pw.appbank.signum.data.apple.str
import pw.appbank.signum.data.models.IpaMetadata
import pw.appbank.signum.data.models.SignOptions
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import java.util.zip.ZipOutputStream

object IpaTweaks {
    fun metadata(ipa: File): IpaMetadata {
        val dict = readInfoPlist(ipa) ?: error("Не удалось прочитать файл приложения")
        val bundle = dict.objectForKey("CFBundleIdentifier")?.toString().orEmpty()
        if (bundle.isEmpty()) error("Файл приложения повреждён")
        val name = dict.objectForKey("CFBundleDisplayName")?.toString()
            ?: dict.objectForKey("CFBundleName")?.toString()
            ?: "App"
        val version = dict.objectForKey("CFBundleShortVersionString")?.toString()
            ?: dict.objectForKey("CFBundleVersion")?.toString()
        val minIos = dict.objectForKey("MinimumOSVersion")?.toString()
        return IpaMetadata(bundle, name, version, minIos)
    }

    fun apply(src: File, dest: File, options: SignOptions) {
        dest.parentFile?.mkdirs()
        ZipFile(src).use { zip ->
            ZipOutputStream(dest.outputStream()).use { out ->
                val entries = zip.entries().toList()
                for (entry in entries) {
                    val name = entry.name.replace('\\', '/')
                    if (shouldDrop(name, options)) continue
                    if (name.startsWith("Payload/") && name.endsWith(".app/Info.plist") &&
                        name.count { it == '/' } == 2
                    ) {
                        val bytes = zip.getInputStream(entry).readBytes()
                        val patched = patchPlist(bytes, options)
                        val ne = ZipEntry(entry.name)
                        out.putNextEntry(ne)
                        out.write(patched)
                        out.closeEntry()
                    } else {
                        val ne = ZipEntry(entry.name)
                        out.putNextEntry(ne)
                        zip.getInputStream(entry).copyTo(out)
                        out.closeEntry()
                    }
                }
            }
        }
    }

    private fun shouldDrop(name: String, options: SignOptions): Boolean {
        if (options.stripWatch && (name.contains("/Watch/") || name.contains(".watchkit"))) return true
        if (options.stripPlugins && name.contains("/PlugIns/")) return true
        if (options.stripFrameworks && name.contains("/Frameworks/")) return true
        return false
    }

    private fun patchPlist(bytes: ByteArray, options: SignOptions): ByteArray {
        val dict = runCatching { Plist.parse(bytes) }.getOrNull() ?: return bytes
        if (options.lowerMinIos) {
            dict.put("MinimumOSVersion", NSString(options.minIosVersion ?: "12.0"))
        }
        if (options.fileSharing) {
            dict.put("UIFileSharingEnabled", true)
            dict.put("LSSupportsOpeningDocumentsInPlace", true)
        }
        if (options.documentsInPlace || options.supportsOpeningDocs) {
            dict.put("LSSupportsOpeningDocumentsInPlace", true)
        }
        if (options.changeAppName && !options.appName.isNullOrBlank()) {
            dict.put("CFBundleDisplayName", NSString(options.appName))
            dict.put("CFBundleName", NSString(options.appName))
        }
        if (options.changeVersion && !options.appVersion.isNullOrBlank()) {
            dict.put("CFBundleShortVersionString", NSString(options.appVersion))
            dict.put("CFBundleVersion", NSString(options.appVersion))
        }
        if (!options.autoBundleId && !options.customBundleId.isNullOrBlank()) {
            dict.put("CFBundleIdentifier", NSString(options.customBundleId))
        }
        if (options.forceUniqueBundle) {
            val current = dict.objectForKey("CFBundleIdentifier")?.toString() ?: "app"
            dict.put("CFBundleIdentifier", NSString("$current.${(1000..9999).random()}"))
        }
        if (options.clearUrlSchemes) dict.remove("CFBundleURLTypes")
        if (options.removeDeviceLimits) {
            dict.remove("UIRequiredDeviceCapabilities")
            dict.put("UIDeviceFamily", listOf(1, 2))
        }
        if (options.allowHttp) {
            val ats = NSDictionary()
            ats.put("NSAllowsArbitraryLoads", true)
            dict.put("NSAppTransportSecurity", ats)
        }
        if (options.removeOrientationLock) {
            dict.remove("UISupportedInterfaceOrientations")
            dict.remove("UISupportedInterfaceOrientations~ipad")
        }
        return Plist.xml(dict)
    }

    /**
     * Пересобирает IPA одним проходом.
     *
     * ipatool не пакует архив заново, а дописывает iTunesMetadata.plist и
     * файл лицензии .sinf в конец готового файла от Apple. Служебная карта
     * META-INF/com.apple.ZipMetadata.plist после этого описывает состояние «до»,
     * и установка на iPhone падает с «Не удаётся установить». Карту выбрасываем —
     * без неё iOS ставит нормально. Содержимое остаётся байт в байт тем же.
     */
    fun repack(path: File) {
        val tmp = File(path.parentFile, path.name + ".repack")
        try {
            ZipFile(path).use { zin ->
                val entries = zin.entries().toList()
                    .filter { !it.isDirectory && !it.name.startsWith("META-INF/") }
                    .sortedWith(compareBy({ it.name != "iTunesMetadata.plist" }, { it.name }))
                ZipOutputStream(tmp.outputStream().buffered()).use { out ->
                    out.setLevel(6)
                    for (entry in entries) {
                        out.putNextEntry(ZipEntry(entry.name).apply { time = entry.time })
                        zin.getInputStream(entry).use { it.copyTo(out) }
                        out.closeEntry()
                    }
                }
            }
            check(tmp.renameTo(path)) { "не удалось заменить ${path.name}" }
        } finally {
            tmp.delete()
        }
    }

    /**
     * Вшивает в скачанный из Store файл персональную лицензию.
     *
     * Apple отдаёт `.sinf` и `iTunesMetadata` отдельно от самой IPA, в теле
     * ответа. Без них файл ставится, но приложение вылетает при запуске: iOS
     * не может получить ключ FairPlay. Десктопный ipatool это делает давно,
     * Android — нет, и именно отсюда жалобы на вылеты.
     *
     * Куда класть `.sinf`, говорит `SC_Info/Manifest.plist`; если его нет,
     * имя берётся из `CFBundleExecutable` — тот же порядок, что на десктопе.
     */
    fun injectStoreLicense(path: File, metadata: ByteArray?, sinfs: List<ByteArray>) {
        require(metadata != null || sinfs.isNotEmpty()) {
            "Apple не вернул лицензию FairPlay для IPA"
        }
        val tmp = File(path.parentFile, path.name + ".lic")
        try {
            ZipFile(path).use { zin ->
                val entries = zin.entries().toList().filter { !it.isDirectory }
                val infoEntry = entries.firstOrNull { e ->
                    val n = e.name.replace('\\', '/')
                    n.startsWith("Payload/") && n.endsWith(".app/Info.plist") && n.count { it == '/' } == 2
                }
                val bundleDir = infoEntry?.name?.replace('\\', '/')?.substringBeforeLast("/Info.plist")

                val extra = LinkedHashMap<String, ByteArray>()
                if (metadata != null) extra["iTunesMetadata.plist"] = metadata

                if (bundleDir != null && sinfs.isNotEmpty()) {
                    val manifest = entries.firstOrNull {
                        it.name.replace('\\', '/') == "$bundleDir/SC_Info/Manifest.plist"
                    }
                    val fromManifest = manifest?.let { m ->
                        runCatching {
                            val d = Plist.parse(zin.getInputStream(m).readBytes())
                            (d.objectForKey("SinfPaths") as? NSArray)
                                ?.array
                                ?.mapNotNull { it?.toString()?.trim() }
                                ?.filter { it.isNotEmpty() }
                                .orEmpty()
                        }.getOrDefault(emptyList())
                    }.orEmpty()
                    val targets = if (fromManifest.isNotEmpty()) {
                        fromManifest.map { "$bundleDir/$it" }
                    } else {
                        val exec = runCatching {
                            Plist.parse(zin.getInputStream(infoEntry).readBytes()).str("CFBundleExecutable")
                        }.getOrNull()
                        if (exec.isNullOrBlank()) emptyList() else listOf("$bundleDir/SC_Info/$exec.sinf")
                    }
                    targets.forEachIndexed { index, target ->
                        if (index < sinfs.size) extra[target] = sinfs[index]
                    }
                }
                check(extra.isNotEmpty()) {
                    "Не удалось определить путь для лицензии FairPlay в IPA"
                }

                ZipOutputStream(tmp.outputStream().buffered()).use { out ->
                    out.setLevel(6)
                    // Свои записи пишем первыми и не копируем одноимённые из
                    // исходника, иначе в архиве окажется два файла с одним путём.
                    for ((name, bytes) in extra) {
                        out.putNextEntry(ZipEntry(name))
                        out.write(bytes)
                        out.closeEntry()
                    }
                    for (entry in entries) {
                        val name = entry.name.replace('\\', '/')
                        if (name in extra || name.startsWith("META-INF/")) continue
                        out.putNextEntry(ZipEntry(entry.name).apply { time = entry.time })
                        zin.getInputStream(entry).use { it.copyTo(out) }
                        out.closeEntry()
                    }
                }
            }
            check(tmp.renameTo(path)) { "не удалось заменить ${path.name}" }
        } finally {
            tmp.delete()
        }
    }

    private fun readInfoPlist(ipa: File): NSDictionary? {
        ZipFile(ipa).use { zip ->
            val entry = zip.entries().toList().firstOrNull { e ->
                val n = e.name.replace('\\', '/')
                n.startsWith("Payload/") && n.endsWith(".app/Info.plist") && n.count { it == '/' } == 2
            } ?: return null
            return Plist.parse(zip.getInputStream(entry).readBytes())
        }
    }
}
