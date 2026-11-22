/**
 * Signum landing: выбор загрузки под ОС, актуальные версии и анимации.
 */
(function () {
  "use strict";

  var reduceMotion = !!(window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches);

  function $all(root, selector) {
    return Array.prototype.slice.call(root.querySelectorAll(selector));
  }

  // ── Загрузки ──────────────────────────────────────────────────────────

  var DOWNLOADS = {
    windows: { id: "windows", label: "Скачать для Windows", shortLabel: "Windows", hint: "Windows 10/11 · версия 1.2.3", url: "download.php?platform=windows", version: "1.2.3", available: true },
    macos: { id: "macos", label: "Скачать для Mac", shortLabel: "Mac", hint: "macOS · Apple Silicon и Intel · версия 1.2.3", url: "download.php?platform=macos", version: "1.2.3", available: true },
    linux: { id: "linux", label: "Скачать для Linux", shortLabel: "Linux", hint: "Linux · AppImage · версия 1.2.3", url: "download.php?platform=linux", version: "1.2.3", available: true, debAvailable: null, debUrl: "download.php?platform=linux&format=deb" },
    android: { id: "android", label: "Скачать для Android", shortLabel: "Android", hint: "Android 10+ · APK · версия 1.2.3", url: "download.php?platform=android", version: "1.2.3", available: true },
  };

  /** @returns {"windows"|"macos"|"linux"|"android"|"unknown"} */
  function detectPlatform() {
    try {
      var uaData = navigator.userAgentData;
      if (uaData && typeof uaData.platform === "string" && uaData.platform) {
        var p = uaData.platform.toLowerCase();
        if (p.indexOf("android") !== -1) return "android";
        if (p.indexOf("win") !== -1) return "windows";
        if (p.indexOf("mac") !== -1) return "macos";
        if (p.indexOf("linux") !== -1 || p.indexOf("chrome os") !== -1) return "linux";
      }
    } catch (_) { /* старый браузер */ }

    var platform = String(navigator.platform || "").toLowerCase();
    var ua = String(navigator.userAgent || "").toLowerCase();
    // iPhone и iPad: своего клиента нет, предлагаем Mac как самый близкий.
    var isIpad = /ipad/.test(ua) || (platform === "macintel" && navigator.maxTouchPoints > 1);
    if (isIpad || /iphone|ipod/.test(ua)) return "macos";
    if (/android/.test(ua)) return "android";
    if (/win/.test(platform) || /windows/.test(ua)) return "windows";
    if (/mac/.test(platform) || /macintosh|mac os x/.test(ua)) return "macos";
    if (/linux/.test(platform) || /linux|cros/.test(ua)) return "linux";
    return "unknown";
  }

  function resolveDownload(platform) {
    return DOWNLOADS[platform] || DOWNLOADS.windows;
  }

  function platformName(platform) {
    return { windows: "Windows", macos: "macOS", linux: "Linux", android: "Android" }[platform] || "Windows";
  }

  function setLinkEnabled(link, enabled, href) {
    if (enabled) {
      if (href) link.setAttribute("href", href);
      link.removeAttribute("aria-disabled");
      link.classList.remove("is-disabled");
      link.onclick = null;
    } else {
      link.removeAttribute("href");
      link.setAttribute("aria-disabled", "true");
      link.classList.add("is-disabled");
      link.onclick = function (event) { event.preventDefault(); };
    }
  }

  /** .deb: null — ещё не знаем (ссылка рабочая), false — сервер сказал, что пакета нет. */
  function applyDebLinks() {
    var linux = DOWNLOADS.linux;
    $all(document, "[data-download-format='deb'], [data-picker-format='deb']").forEach(function (link) {
      var available = linux.debAvailable !== false;
      setLinkEnabled(link, available, linux.debUrl);
      var status = link.querySelector("[data-deb-status], [data-deb-picker-status]");
      if (status) status.textContent = available ? (status.hasAttribute("data-deb-status") ? "Ubuntu / Debian" : "") : "сейчас недоступен";
    });
  }

  function applyDownloadUI(platform) {
    var pack = resolveDownload(platform);
    var detected = platform === "unknown" ? "windows" : platform;

    $all(document, "[data-download-primary]").forEach(function (el) {
      el.setAttribute("href", pack.url);
      el.setAttribute("download", "");
    });
    $all(document, "[data-download-label]").forEach(function (el) {
      el.textContent = pack.label;
    });

    var hint = document.getElementById("download-hint");
    if (hint) hint.textContent = pack.hint;

    var osChip = document.getElementById("os-chip");
    if (osChip) osChip.textContent = platform === "unknown" ? "Windows" : "У вас " + (platform === "macos" ? "Mac" : platformName(platform));

    var versionChip = document.getElementById("version-chip");
    if (versionChip) versionChip.textContent = "v" + pack.version;

    $all(document, "[data-platform-card]").forEach(function (card) {
      card.classList.toggle("is-active", card.getAttribute("data-platform-card") === detected);
    });
    $all(document, "[data-platform-badge]").forEach(function (badge) {
      badge.hidden = badge.getAttribute("data-platform-badge") !== detected;
    });

    // Диалог «Все платформы».
    $all(document, "[data-picker-detected]").forEach(function (node) { node.textContent = platformName(detected); });
    $all(document, "[data-picker-platform]").forEach(function (row) {
      var id = row.getAttribute("data-picker-platform");
      row.classList.toggle("is-detected", id === detected);
      var badge = row.querySelector("[data-picker-badge]");
      if (badge) badge.hidden = id !== detected;
    });

    $all(document, "[data-download-link], [data-picker-download-link]").forEach(function (link) {
      if (link.hasAttribute("data-download-format") || link.hasAttribute("data-picker-format")) return;
      var conf = DOWNLOADS[link.getAttribute("data-download-link") || link.getAttribute("data-picker-download-link")];
      if (!conf) return;
      link.setAttribute("href", conf.url);
      link.setAttribute("download", "");
    });

    applyDebLinks();
  }

  /**
   * Подписи версий из version.php. Числа в разметке — запасной вариант на
   * случай, если запрос не прошёл: страница могла закешироваться до релиза.
   */
  function refreshVersions(platform) {
    if (typeof fetch !== "function") return;
    fetch("version.php?_=" + Date.now(), { cache: "no-store", credentials: "same-origin" })
      .then(function (response) { return response.ok ? response.json() : null; })
      .then(function (data) {
        if (!data || !data.ok || !data.platforms) return;
        Object.keys(DOWNLOADS).forEach(function (key) {
          var fresh = data.platforms[key];
          if (!fresh || !fresh.version) return;
          var pack = DOWNLOADS[key];
          pack.hint = pack.hint.replace(pack.version, fresh.version);
          pack.version = fresh.version;
          if (fresh.download_url) pack.url = fresh.download_url;
          if (key === "linux") {
            pack.debAvailable = fresh.deb_available === true && !!fresh.deb_download_url;
            if (pack.debAvailable) pack.debUrl = fresh.deb_download_url;
          }
        });
        $all(document, "[data-version]").forEach(function (node) {
          var fresh = data.platforms[node.getAttribute("data-version")];
          if (fresh && fresh.version) node.textContent = fresh.version;
        });
        applyDownloadUI(platform);
        renderChecksums(data.platforms);
      })
      .catch(function () { /* остаются значения из разметки */ });
  }

  // ── Контрольные суммы SHA-256 ─────────────────────────────────────────

  var CHECKSUM_NAMES = [
    ["windows", "Windows", "bi-windows"],
    ["macos", "macOS", "bi-apple"],
    ["linux", "Linux · AppImage", "bi-ubuntu"],
    ["android", "Android", "bi-android2"]
  ];

  function copyText(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) return navigator.clipboard.writeText(text);
    return new Promise(function (resolve, reject) {
      var area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "");
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      var ok = false;
      try { ok = document.execCommand("copy"); } catch (e) { ok = false; }
      document.body.removeChild(area);
      if (ok) resolve(); else reject(new Error("copy failed"));
    });
  }

  function renderChecksums(platforms) {
    var box = document.getElementById("checksums");
    var list = box && box.querySelector("[data-checksums-list]");
    if (!list) return;
    list.textContent = "";
    var count = 0;
    CHECKSUM_NAMES.forEach(function (entry) {
      var info = platforms[entry[0]];
      var hash = info && typeof info.sha256 === "string" ? info.sha256.toLowerCase() : "";
      if (!/^[a-f0-9]{64}$/.test(hash)) return;
      var row = document.createElement("li");
      row.className = "checksum";
      var icon = document.createElement("i");
      icon.className = "bi " + entry[2];
      icon.setAttribute("aria-hidden", "true");
      var name = document.createElement("span");
      name.className = "checksum-name";
      name.textContent = entry[1];
      if (info.version) {
        var ver = document.createElement("small");
        ver.textContent = "v" + info.version;
        name.appendChild(ver);
      }
      var code = document.createElement("code");
      code.className = "checksum-hash";
      code.textContent = hash;
      var button = document.createElement("button");
      button.type = "button";
      button.className = "checksum-copy";
      button.setAttribute("aria-label", "Скопировать отпечаток для " + entry[1]);
      button.innerHTML = '<i class="bi bi-copy" aria-hidden="true"></i><span>Копировать</span>';
      var timer = 0;
      button.addEventListener("click", function () {
        copyText(hash).then(function () {
          button.classList.add("is-done");
          button.lastChild.textContent = "Скопировано";
          window.clearTimeout(timer);
          timer = window.setTimeout(function () {
            button.classList.remove("is-done");
            button.lastChild.textContent = "Копировать";
          }, 1800);
        }).catch(function () {
          var range = document.createRange();
          range.selectNodeContents(code);
          var sel = window.getSelection();
          sel.removeAllRanges();
          sel.addRange(range);
        });
      });
      row.appendChild(icon);
      row.appendChild(name);
      row.appendChild(code);
      row.appendChild(button);
      list.appendChild(row);
      count += 1;
    });
    box.hidden = count === 0;
    if (count) box.classList.add("is-visible");
  }

  // ── Диалог «Все платформы» ────────────────────────────────────────────

  function initPlatformPicker() {
    var dialog = document.getElementById("platform-picker");
    var triggers = $all(document, "[data-platform-picker-open]");
    if (!dialog || !triggers.length) return;
    if (typeof dialog.showModal !== "function") {
      // Без <dialog> просто ведём к блоку загрузок.
      triggers.forEach(function (t) {
        t.addEventListener("click", function () { location.hash = "#download"; });
      });
      return;
    }
    var opener = null;
    var closeTimer = 0;

    function finishClose() {
      dialog.classList.remove("is-open");
      if (dialog.open) dialog.close();
    }
    function closeDialog() {
      if (!dialog.open) return;
      dialog.classList.remove("is-open");
      window.clearTimeout(closeTimer);
      closeTimer = window.setTimeout(finishClose, reduceMotion ? 0 : 220);
    }
    function openDialog(event) {
      opener = event.currentTarget;
      if (dialog.open) return;
      window.clearTimeout(closeTimer);
      dialog.showModal();
      requestAnimationFrame(function () { dialog.classList.add("is-open"); });
    }

    triggers.forEach(function (t) { t.addEventListener("click", openDialog); });
    $all(dialog, "[data-platform-picker-close]").forEach(function (b) { b.addEventListener("click", closeDialog); });
    dialog.addEventListener("click", function (event) { if (event.target === dialog) closeDialog(); });
    dialog.addEventListener("cancel", function (event) { event.preventDefault(); closeDialog(); });
    dialog.addEventListener("close", function () {
      dialog.classList.remove("is-open");
      if (opener && document.contains(opener)) opener.focus();
    });
    $all(dialog, "a[href]").forEach(function (link) { link.addEventListener("click", closeDialog); });
  }

  // ── Шапка и меню ──────────────────────────────────────────────────────

  function initHeader() {
    var header = document.getElementById("site-header");
    if (!header) return;
    var onScroll = function () { header.classList.toggle("is-scrolled", window.scrollY > 8); };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });

    // Текущий раздел подчёркнут в меню.
    var links = $all(document, ".nav a[href^='#']");
    if (!links.length || !("IntersectionObserver" in window)) return;
    var byId = {};
    links.forEach(function (a) { byId[a.getAttribute("href").slice(1)] = a; });
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        var link = byId[entry.target.id];
        if (!link || !entry.isIntersecting) return;
        links.forEach(function (a) { a.classList.toggle("is-current", a === link); });
      });
    }, { rootMargin: "-45% 0px -50% 0px" });
    Object.keys(byId).forEach(function (id) {
      var section = document.getElementById(id);
      if (section) io.observe(section);
    });
  }

  function initMobileNav() {
    var toggle = document.querySelector(".nav-toggle");
    var panel = document.getElementById("mobile-nav");
    if (!toggle || !panel) return;
    var closeTimer = 0;

    function setExpanded(open) {
      toggle.setAttribute("aria-expanded", open ? "true" : "false");
      toggle.setAttribute("aria-label", open ? "Закрыть меню" : "Открыть меню");
    }
    function open() {
      window.clearTimeout(closeTimer);
      panel.hidden = false;
      requestAnimationFrame(function () {
        requestAnimationFrame(function () { panel.classList.add("is-open"); });
      });
      setExpanded(true);
    }
    function close() {
      if (panel.hidden) return;
      panel.classList.remove("is-open");
      setExpanded(false);
      window.clearTimeout(closeTimer);
      closeTimer = window.setTimeout(function () { panel.hidden = true; }, reduceMotion ? 0 : 380);
    }
    toggle.addEventListener("click", function () {
      if (panel.classList.contains("is-open")) close(); else open();
    });
    $all(panel, "a").forEach(function (a) { a.addEventListener("click", close); });
    window.addEventListener("resize", function () { if (window.innerWidth > 760) close(); });
    document.addEventListener("keydown", function (e) { if (e.key === "Escape") close(); });
  }

  // ── Появление при прокрутке ───────────────────────────────────────────

  function initReveal() {
    var nodes = $all(document, ".reveal");
    // Соседи появляются лесенкой, а не все разом.
    nodes.forEach(function (node) {
      var siblings = Array.prototype.filter.call(node.parentNode.children, function (el) {
        return el.classList.contains("reveal");
      });
      var index = siblings.indexOf(node);
      if (index > 0) node.style.setProperty("--d", Math.min(index, 6) * 0.07 + "s");
    });
    if (reduceMotion || !("IntersectionObserver" in window)) {
      nodes.forEach(function (n) { n.classList.add("is-visible"); });
      return;
    }
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (!entry.isIntersecting) return;
        entry.target.classList.add("is-visible");
        io.unobserve(entry.target);
      });
    }, { rootMargin: "0px 0px -8% 0px", threshold: 0.1 });
    nodes.forEach(function (n) { io.observe(n); });
  }

  // ── Скриншот и шаги: прогресс от прокрутки ────────────────────────────

  function initScrollScenes() {
    var shot = document.querySelector("[data-shot] img");
    var steps = $all(document, ".step");
    var rail = document.querySelector("[data-steps-rail]");
    var stepsBox = document.querySelector("[data-steps]");
    if (reduceMotion) {
      if (shot) shot.style.setProperty("--tilt", "0");
      steps.forEach(function (s) { s.classList.add("is-lit"); });
      if (rail) rail.style.setProperty("--progress", "1");
      return;
    }
    var ticking = false;
    function clamp(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }
    function update() {
      ticking = false;
      var vh = window.innerHeight;
      if (shot) {
        var r = shot.getBoundingClientRect();
        // 1 — у нижнего края экрана, 0 — когда верх картинки на трети высоты.
        var tilt = clamp((r.top - vh * 0.3) / (vh * 0.65));
        shot.style.setProperty("--tilt", tilt.toFixed(3));
      }
      if (stepsBox && steps.length) {
        var b = stepsBox.getBoundingClientRect();
        var progress = clamp((vh * 0.75 - b.top) / (b.height + vh * 0.25));
        if (rail) rail.style.setProperty("--progress", progress.toFixed(3));
        steps.forEach(function (step, i) {
          step.classList.toggle("is-lit", progress >= (i + 0.35) / steps.length || (window.innerWidth <= 900 && step.getBoundingClientRect().top < vh * 0.7));
        });
      }
    }
    function onScroll() {
      if (!ticking) { ticking = true; requestAnimationFrame(update); }
    }
    update();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
  }

  // ── FAQ: плавное раскрытие ────────────────────────────────────────────

  function initFaq() {
    $all(document, ".faq-item").forEach(function (details) {
      var summary = details.querySelector("summary");
      var panel = details.querySelector(".faq-panel");
      if (!summary || !panel) return;
      panel.classList.add("is-js");
      details.classList.toggle("is-open", details.open);
      panel.style.height = details.open ? "auto" : "0px";
      var token = 0;

      summary.addEventListener("click", function (e) {
        e.preventDefault();
        var current = ++token;
        if (details.classList.contains("is-open")) {
          details.classList.remove("is-open");
          panel.style.height = panel.scrollHeight + "px";
          void panel.offsetHeight;
          panel.style.height = "0px";
          window.setTimeout(function () { if (current === token) details.open = false; }, reduceMotion ? 0 : 420);
        } else {
          details.open = true;
          details.classList.add("is-open");
          panel.style.height = "0px";
          void panel.offsetHeight;
          panel.style.height = panel.scrollHeight + "px";
          window.setTimeout(function () { if (current === token) panel.style.height = "auto"; }, reduceMotion ? 0 : 420);
        }
      });
    });
  }

  // ── Демо в первом экране ──────────────────────────────────────────────
  //
  // Поиск сразу по каталогу и App Store: одно и то же приложение
  // показывается одной карточкой, затем установка на iPhone.

  var SCENES = [
    {
      query: "вк",
      catalog: [
        { title: "ВКонтакте", meta: "проверенная сборка", color: "#0077ff", letter: "VK", storeId: "vk" },
        { title: "ВК Видео", meta: "проверенная сборка", color: "#0077ff", letter: "▶", storeId: "vk-video" },
        { title: "ВК Музыка", meta: "проверенная сборка", color: "#0077ff", letter: "♪", storeId: "vk-music" },
      ],
      store: [
        { title: "VK: music, video, messenger", meta: "App Store", color: "#0077ff", letter: "VK", storeId: "vk" },
        { title: "VK Video: films, series", meta: "App Store", color: "#0077ff", letter: "▶", storeId: "vk-video" },
        { title: "Kate Mobile", meta: "App Store", color: "#5b6f8c", letter: "K" },
      ],
      install: "ВКонтакте",
    },
    {
      query: "сбер",
      catalog: [
        { title: "СберБанк Онлайн", meta: "проверенная сборка", color: "#21a038", letter: "С" },
        { title: "Сбер Инвестиции", meta: "проверенная сборка", color: "#21a038", letter: "И" },
        { title: "СберБизнес", meta: "проверенная сборка", color: "#107f8c", letter: "Б" },
      ],
      store: [
        { title: "СберKids", meta: "в покупках", color: "#21a038", letter: "K" },
        { title: "СберМобайл", meta: "App Store", color: "#21a038", letter: "М" },
      ],
      install: "СберБанк Онлайн",
    },
  ];

  function initDemo() {
    var root = document.querySelector("[data-demo]");
    if (!root) return;
    var q = root.querySelector("[data-demo-query]");
    var catalogBox = root.querySelector("[data-demo-catalog]");
    var storeBox = root.querySelector("[data-demo-store]");
    var count = root.querySelector("[data-demo-count]");
    var storeCount = root.querySelector("[data-demo-store-count]");
    var install = root.querySelector("[data-demo-install]");
    var installLabel = root.querySelector("[data-demo-install-label]");
    var installPct = root.querySelector("[data-demo-install-pct]");
    var bar = root.querySelector("[data-demo-progress]");

    var timers = [];
    var running = false;
    var sceneIndex = 0;

    function later(fn, ms) { timers.push(window.setTimeout(fn, ms)); }
    function clearTimers() { timers.forEach(window.clearTimeout); timers = []; }

    function icon(item) {
      var el = document.createElement("span");
      el.className = "demo-icon";
      el.style.background = item.color;
      el.textContent = item.letter;
      return el;
    }
    function card(item) {
      var el = document.createElement("div");
      el.className = "demo-card";
      var b = document.createElement("b");
      b.textContent = item.title;
      var s = document.createElement("small");
      s.textContent = item.meta;
      el.appendChild(icon(item));
      el.appendChild(b);
      el.appendChild(s);
      if (item.storeId) el.setAttribute("data-store-id", item.storeId);
      return el;
    }
    function row(item) {
      var el = document.createElement("div");
      el.className = "demo-row";
      var t = document.createElement("span");
      t.textContent = item.title;
      var m = document.createElement("small");
      m.textContent = item.meta;
      el.appendChild(icon(item));
      el.appendChild(t);
      el.appendChild(m);
      if (item.storeId) el.setAttribute("data-store-id", item.storeId);
      return el;
    }

    function reset() {
      q.textContent = "";
      catalogBox.innerHTML = "";
      storeBox.innerHTML = "";
      count.textContent = "0";
      storeCount.textContent = "0";
      install.classList.remove("is-on", "is-done");
      bar.style.width = "0%";
      installPct.textContent = "0%";
    }

    /** Финальный кадр без движения — для prefers-reduced-motion. */
    function renderStatic(scene) {
      reset();
      q.textContent = scene.query;
      scene.catalog.forEach(function (item) { catalogBox.appendChild(card(item)); });
      var merged = {};
      scene.catalog.forEach(function (item) { if (item.storeId) merged[item.storeId] = true; });
      var left = scene.store.filter(function (item) { return !item.storeId || !merged[item.storeId]; });
      left.forEach(function (item) { storeBox.appendChild(row(item)); });
      count.textContent = String(scene.catalog.length);
      storeCount.textContent = String(left.length);
      install.classList.add("is-on", "is-done");
      installLabel.textContent = scene.install + " — на iPhone";
      installPct.textContent = "готово";
      bar.style.width = "100%";
    }

    function play() {
      var scene = SCENES[sceneIndex % SCENES.length];
      sceneIndex += 1;
      reset();
      var t = 300;
      // Печать запроса.
      scene.query.split("").forEach(function (ch, i) {
        later(function () { q.textContent += ch; }, t + i * 170);
      });
      t += scene.query.length * 170 + 250;
      // Каталог отвечает мгновенно.
      scene.catalog.forEach(function (item, i) {
        later(function () {
          catalogBox.appendChild(card(item));
          count.textContent = String(i + 1);
        }, t + i * 110);
      });
      t += scene.catalog.length * 110 + 500;
      // App Store отвечает позже.
      scene.store.forEach(function (item, i) {
        later(function () {
          storeBox.appendChild(row(item));
          storeCount.textContent = String(i + 1);
        }, t + i * 150);
      });
      t += scene.store.length * 150 + 900;
      // Совпадения из App Store сливаются с карточками каталога.
      later(function () {
        var left = scene.store.length;
        $all(storeBox, ".demo-row").forEach(function (r) {
          var id = r.getAttribute("data-store-id");
          if (!id) return;
          var twin = catalogBox.querySelector("[data-store-id='" + id + "']");
          if (!twin) return;
          left -= 1;
          r.classList.add("is-leaving");
          twin.classList.add("is-merged");
          var tag = document.createElement("span");
          tag.className = "demo-tag";
          tag.textContent = "= App Store";
          twin.appendChild(tag);
        });
        storeCount.textContent = String(left);
      }, t);
      t += 1300;
      // Установка.
      var phases = [
        [0, "Получаем в Apple ID"],
        [18, "Скачиваем приложение"],
        [72, "Ставим на iPhone"],
      ];
      later(function () {
        install.classList.add("is-on");
        installLabel.textContent = phases[0][1] + " · " + scene.install;
      }, t);
      t += 300;
      for (var p = 0; p <= 100; p += 4) {
        (function (pct, at) {
          later(function () {
            bar.style.width = pct + "%";
            installPct.textContent = pct + "%";
            for (var k = phases.length - 1; k >= 0; k--) {
              if (pct >= phases[k][0]) { installLabel.textContent = phases[k][1] + " · " + scene.install; break; }
            }
          }, at);
        })(p, t + p * 26);
      }
      t += 100 * 26 + 200;
      later(function () {
        install.classList.add("is-done");
        installLabel.textContent = scene.install + " — на iPhone";
        installPct.textContent = "готово";
      }, t);
      t += 2600;
      later(play, t);
    }

    if (reduceMotion) {
      renderStatic(SCENES[0]);
      return;
    }

    function start() { if (!running) { running = true; play(); } }
    function stop() { running = false; clearTimers(); }

    // Не крутим анимацию, когда окно демо не видно или вкладка в фоне.
    if ("IntersectionObserver" in window) {
      new IntersectionObserver(function (entries) {
        entries.forEach(function (entry) { if (entry.isIntersecting) start(); else stop(); });
      }, { threshold: 0.2 }).observe(root);
    } else {
      start();
    }
    document.addEventListener("visibilitychange", function () {
      if (document.hidden) stop();
      else if (root.getBoundingClientRect().top < window.innerHeight) start();
    });
  }

  function initYear() {
    var el = document.getElementById("year");
    if (el) el.textContent = String(new Date().getFullYear());
  }

  function init() {
    window.__signumReady = true;
    var platform = detectPlatform();
    applyDownloadUI(platform);
    refreshVersions(platform);
    initHeader();
    initMobileNav();
    initReveal();
    initScrollScenes();
    initFaq();
    initPlatformPicker();
    initDemo();
    initYear();
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();
})();
