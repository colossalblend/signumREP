package pw.appbank.signum.data.jobs

import java.io.File

/** Each job owns one explicit IPA path, independent of Store metadata. */
object CatalogDownloadFiles {
    fun output(cacheDir: File, jobId: String): File {
        require(jobId.isNotEmpty() && jobId.all { it.isLetterOrDigit() || it == '-' }) {
            "Некорректный номер задачи загрузки."
        }
        val dir = File(cacheDir, "temp/signum-$jobId")
        check(dir.isDirectory || dir.mkdirs()) { "Не удалось создать папку загрузки." }
        return File(dir, "download.ipa")
    }

    fun cleanup(ipa: File) {
        // Delete only this file and an empty job folder. Preserve failed jobs
        // and any partial files that can be resumed or used for diagnostics.
        if (!ipa.exists() || ipa.delete()) ipa.parentFile?.delete()
    }
}
