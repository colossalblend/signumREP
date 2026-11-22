# Пересборка IPA для установки по ссылке (itms-services)

Техническое задание. Проверено на живом устройстве: iPhone X, iOS 16.7.14,
приложение из App Store (`com.paata.shihan`, 216 МБ). До пересборки —
«Не удаётся установить», после — ставится.

## Симптом

Установка по манифесту `.plist` доходит до конца и падает:

- `appstored` скачивает `.plist` — 200;
- делает `HEAD`, затем `GET` на `.ipa` — 200, файл уходит целиком;
- iPhone показывает **«Не удаётся установить <App>. Повторите попытку позже»**.

То есть ни сеть, ни манифест, ни сертификат ни при чём — падает сама установка.

## Что именно ломается

`ipatool` (и всё, что построено на нём) не собирает IPA заново, а **дописывает
в конец готового архива** две вещи:

```
iTunesMetadata.plist
Payload/<App>.app/SC_Info/<App>.sinf
```

Внутри архива от Apple лежит служебная папка:

```
META-INF/com.apple.ZipMetadata.plist
META-INF/com.apple.FixedZipMetadata.bin
```

Это карта архива для быстрой распаковки: количество записей, суммарный размер,
чем и как паковали. После дозаписи она описывает **состояние до неё** — в нашем
файле `RecordCount: 60` при 65 фактических записях. Установщик на устройстве
идёт по этой карте и спотыкается.

Проверка на своём файле (Python):

```python
import plistlib, zipfile
z = zipfile.ZipFile("app.ipa")
meta = plistlib.loads(z.read("META-INF/com.apple.ZipMetadata.plist"))
print(meta["RecordCount"], "против", len(z.namelist()))   # 60 против 65 — сломано
```

## Что нужно сделать

Переписать архив **одним проходом**, до того как отдавать его по ссылке:

1. Читать все записи исходного IPA (каталоги пропускать).
2. **Не переносить** ничего из `META-INF/` — устаревшую карту выбрасываем целиком,
   заново её строить не нужно, установщик прекрасно обходится без неё.
3. `iTunesMetadata.plist` записать **первым**, остальное — следом.
4. Сжатие `deflate`, уровень 6 (уровень 1 даёт тот же размер и выигрывает
   пару секунд — можно взять и его).
5. Заменить исходный файл результатом, **имя не менять**: на него уже ссылается
   манифест.

Ничего расшифровывать, подписывать или править внутри не нужно — содержимое
байт в байт то же самое, меняется только упаковка.

## Куда встроить

Между «скачали IPA» и «сформировали `.plist`». Только для установки по ссылке;
для выдачи файла пользователю (Signum по USB, AltStore, Sideloadly) пересборка
не нужна — там всё работает и так.

Операция упирается в процессор: **17 секунд на 216 МБ**. Уводите её с основного
потока и покажите шаг в прогрессе, иначе интерфейс замирает.

## Реализация

### Rust (`zip = "2"`, уже есть в `src-tauri/Cargo.toml`)

```rust
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

/// Пересобирает IPA: иначе iPhone скачает файл и откажется его ставить.
pub fn repack_ipa(path: &Path) -> anyhow::Result<()> {
    let tmp = path.with_extension("repack");
    {
        let mut zin = ZipArchive::new(File::open(path)?)?;
        let mut zout = ZipWriter::new(File::create(&tmp)?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6));

        // Метаданные покупки должны идти первыми, служебную карту Apple выбрасываем.
        let mut order: Vec<usize> = (0..zin.len()).collect();
        order.sort_by_key(|&i| {
            let name = zin.name_for_index(i).unwrap_or("").to_string();
            (name != "iTunesMetadata.plist", name)
        });

        for i in order {
            let mut entry = zin.by_index(i)?;
            let name = entry.name().to_string();
            if entry.is_dir() || name.starts_with("META-INF/") {
                continue;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)?;
            zout.start_file(name, options)?;
            zout.write_all(&buf)?;
        }
        zout.finish()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

Вызывать вне UI-потока:

```rust
let ipa = ipa.clone();
tokio::task::spawn_blocking(move || repack_ipa(&ipa)).await??;
```

### Kotlin (`java.util.zip`, ничего добавлять не нужно)

```kotlin
/** Пересобирает IPA: иначе iPhone скачает файл и откажется его ставить. */
fun repackIpa(path: File) {
    val tmp = File(path.parentFile, path.name + ".repack")
    ZipFile(path).use { zin ->
        val entries = zin.entries().asSequence()
            .filter { !it.isDirectory && !it.name.startsWith("META-INF/") }
            .sortedWith(compareBy({ it.name != "iTunesMetadata.plist" }, { it.name }))
            .toList()
        ZipOutputStream(tmp.outputStream().buffered()).use { out ->
            out.setLevel(6)
            for (entry in entries) {
                out.putNextEntry(ZipEntry(entry.name).apply { time = entry.time })
                zin.getInputStream(entry).use { it.copyTo(out) }
                out.closeEntry()
            }
        }
    }
    check(tmp.renameTo(path)) { "не удалось заменить $path" }
}
```

Вызывать в `Dispatchers.IO`.

### Python (эталон, из бота)

```python
def repack(path: Path) -> Path:
    tmp = path.with_suffix(".repack")
    with zipfile.ZipFile(path) as zin:
        items = [i for i in zin.infolist() if not i.is_dir()]
        items.sort(key=lambda i: (0 if i.filename == "iTunesMetadata.plist" else 1, i.filename))
        with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as zout:
            for item in items:
                if item.filename.startswith("META-INF/"):
                    continue
                info = zipfile.ZipInfo(item.filename, date_time=item.date_time)
                info.external_attr = item.external_attr
                info.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(info, zin.read(item.filename))
    shutil.move(str(tmp), path)
    return path
```

## Как убедиться, что вышло правильно

```python
import zipfile
z = zipfile.ZipFile("app.ipa")
names = z.namelist()
assert names[0] == "iTunesMetadata.plist"
assert not any(n.startswith("META-INF") for n in names)
assert any(n.endswith(".sinf") for n in names)      # лицензию терять нельзя
```

Размер меняется на десятые доли процента — это нормально, содержимое то же.

## Чего пересборка не делает

Она чинит **упаковку**, а не права. Всё остальное по-прежнему обязательно:

- на iPhone в App Store должна быть включена **та учётная запись, которой
  принадлежит покупка** — сборка зашифрована FairPlay под неё;
- манифест раздаётся по **HTTPS с обычным доверенным сертификатом**;
  самоподписанный, даже установленный на устройство, не годится;
- `bundle-identifier` и `bundle-version` в манифесте берите **из самой сборки**
  (`Payload/*.app/Info.plist`), а не из своего каталога — они расходятся;
- в манифесте оставьте один ресурс `software-package`. Картинки
  (`display-image`, `full-size-image`) iOS тянет во время установки, и сбой на
  их адресе валит установку целиком.
