/**
 * Signum landing — OS detection, smart download, UI interactions.
 * No build step; plain ES5-safe modern JS.
 */
(function () {
  "use strict";

  /** @typedef {"windows"|"macos"|"linux"|"android"|"unknown"} Platform */

  var DOWNLOADS = {
    windows: {
      id: "windows",
      label: "Скачать для Windows",
      shortLabel: "Windows",
      hint: "Windows 10/11 · версия 1.2.3",
      url: "download.php?platform=windows",
      version: "1.2.3",
      available: true,
    },
    macos: {
      id: "macos",
      label: "Скачать для Mac",
      shortLabel: "Mac",
      hint: "Mac · версия 1.2.3",
      url: "download.php?platform=macos",
      version: "1.2.3",
      available: true,
    },
    linux: {
      id: "linux",
      label: "Скачать для Linux",
      shortLabel: "Linux",
      hint: "Linux · AppImage · версия 1.2.3",
      url: "download.php?platform=linux",
      version: "1.2.3",
      available: true,
    },
    android: {
      id: "android",
      label: "Скачать для Android",
      shortLabel: "Android",
      hint: "Android · APK · версия 1.2.3",
      url: "download.php?platform=android",
      version: "1.2.3",
      available: true,
    },
  };

  var FALLBACK = DOWNLOADS.windows;

  /**
   * Detect desktop OS for download targeting.
   * Priority: userAgentData.platform → navigator.platform → userAgent.
   * iOS/iPadOS treated as macos family for CTA messaging only (no mobile app).
   * @returns {Platform}
   */
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
    } catch (_) {
      /* ignore */
    }

    var platform = String(navigator.platform || "").toLowerCase();
    var ua = String(navigator.userAgent || "").toLowerCase();

    // iPadOS 13+ reports as Mac — still not a desktop installer target
    var isIpad = /ipad/.test(ua) || (platform === "macintel" && navigator.maxTouchPoints > 1);
    var isIphone = /iphone|ipod/.test(ua);
    if (isIpad || isIphone) return "macos";

    if (/android/.test(ua)) return "android";
    if (/win/.test(platform) || /windows/.test(ua)) return "windows";
    if (/mac/.test(platform) || /macintosh|mac os x/.test(ua)) return "macos";
    if (/linux/.test(platform) || /linux|cros/.test(ua)) return "linux";

    return "unknown";
  }

  /**
   * @param {Platform} platform
   */
  function resolveDownload(platform) {
    if (platform === "macos") return DOWNLOADS.macos;
    if (platform === "windows") return DOWNLOADS.windows;
    if (platform === "linux") return DOWNLOADS.linux;
    if (platform === "android") return DOWNLOADS.android;
    return DOWNLOADS.windows;
  }

  /**
   * @param {HTMLElement|Document} root
   * @param {string} selector
   */
  function $all(root, selector) {
    return Array.prototype.slice.call(root.querySelectorAll(selector));
  }

  function platformName(platform) {
    if (platform === "windows") return "Windows";
    if (platform === "macos") return "macOS";
    if (platform === "linux") return "Linux";
    if (platform === "android") return "Android";
    return "Windows";
  }

  function updatePlatformPicker(platform) {
    var detected = platform === "unknown" ? "windows" : platform;

    $all(document, "[data-picker-detected]").forEach(function (node) {
      node.textContent = platformName(detected);
    });

    $all(document, "[data-picker-platform]").forEach(function (option) {
      var id = option.getAttribute("data-picker-platform");
      var conf = DOWNLOADS[id];
      if (!conf) return;

      option.classList.toggle("is-detected", id === detected);

      var badge = option.querySelector("[data-picker-badge]");
      if (badge) badge.hidden = id !== detected;

      var status = option.querySelector(".platform-status");
      if (status) {
        status.textContent = conf.available ? "Доступно" : "Готовится";
        status.classList.toggle("is-ready", conf.available);
        status.classList.toggle("is-soon", !conf.available);
      }

      var link = option.querySelector("[data-picker-download-link]");
      var soon = option.querySelector("[data-picker-soon]");
      var isLinux = id === "linux";

      if (isLinux) {

        $all(option, "[data-picker-download-link]").forEach(function (formatLink) {
          var isDeb = formatLink.hasAttribute("data-picker-format");
        if (isDeb) {
            if (conf.debAvailable && conf.debUrl) {
              formatLink.setAttribute("href", conf.debUrl);
              formatLink.removeAttribute("aria-disabled");
              formatLink.onclick = null;
            } else {
              formatLink.removeAttribute("href");
              formatLink.setAttribute("aria-disabled", "true");
              formatLink.onclick = function (event) { event.preventDefault(); };
            }
            var status = formatLink.querySelector("[data-deb-status], [data-deb-picker-status]");
            if (status && !conf.debAvailable) status.textContent = "Сейчас недоступен";
          } else {
            formatLink.setAttribute("href", conf.url);
          }
          var available = conf.available && (!isDeb || conf.debAvailable === true);

          formatLink.hidden = false;

          if (available) {
            formatLink.removeAttribute("aria-disabled");
            formatLink.onclick = null;
          } else {
            formatLink.setAttribute("aria-disabled", "true");
            formatLink.onclick = function (event) { event.preventDefault(); };
          }
        });

      }

      if (link && !isLinux) {
        link.setAttribute("href", conf.url);
        link.hidden = !conf.available;
        if (conf.available) link.setAttribute("download", "");
        else link.removeAttribute("download");
      }
      if (soon) soon.hidden = conf.available;
    });
  }

  function initPlatformPicker() {
    var dialog = document.getElementById("platform-picker");
    var triggers = $all(document, "[data-platform-picker-open]");
    var closeButtons = $all(dialog || document, "[data-platform-picker-close]");
    if (!dialog || !triggers.length) return;

    var opener = null;
    var closing = false;
    var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    var finishClose = function () {
      dialog.classList.remove("is-closing", "is-open");
      closing = false;
      if (typeof dialog.close === "function") dialog.close();
      else dialog.removeAttribute("open");
    };

    var closeDialog = function () {
      if (!dialog.open || closing) return;
      if (reduce) {
        finishClose();
        return;
      }
      closing = true;
      dialog.classList.remove("is-open");
      dialog.classList.add("is-closing");
      window.setTimeout(finishClose, 180);
    };

    var openDialog = function (event) {
      opener = event.currentTarget;
      if (dialog.open) return;
      if (typeof dialog.showModal === "function") dialog.showModal();
      else dialog.setAttribute("open", "");
      if (reduce) {
        dialog.classList.add("is-open");
        return;
      }
      requestAnimationFrame(function () {
        dialog.classList.add("is-open");
      });
    };

    triggers.forEach(function (trigger) {
      trigger.addEventListener("click", openDialog);
    });

    closeButtons.forEach(function (button) {
      button.addEventListener("click", closeDialog);
    });

    dialog.addEventListener("click", function (event) {
      if (event.target === dialog) closeDialog();
    });

    dialog.addEventListener("cancel", function (event) {
      event.preventDefault();
      closeDialog();
    });

    dialog.addEventListener("close", function () {
      dialog.classList.remove("is-closing", "is-open");
      closing = false;
      if (opener && document.contains(opener)) opener.focus();
    });

    $all(dialog, "[data-picker-download-link]").forEach(function (link) {
      link.addEventListener("click", function () {
        closeDialog();
      });
    });
  }

  function applyDownloadUI(platform) {
    var pack = resolveDownload(platform);
    var primaryPack = pack.available ? pack : FALLBACK;

    // Primary CTAs
    $all(document, "[data-download-primary]").forEach(function (el) {
      el.setAttribute("href", primaryPack.url);
      if (primaryPack.available) {
        el.setAttribute("download", "");
        el.removeAttribute("aria-disabled");
        el.classList.remove("is-disabled");
      } else {
        el.removeAttribute("download");
        el.setAttribute("aria-disabled", "true");
        el.classList.add("is-disabled");
      }
    });

    $all(document, "[data-download-label]").forEach(function (el) {
      el.textContent = primaryPack.label;
    });

    var hint = document.getElementById("download-hint");
    if (hint) {
      if (!pack.available && platform !== "unknown") {
        hint.textContent = "Версия для " + pack.shortLabel + " готовится. Пока доступна Windows · v" + FALLBACK.version;
      } else {
        hint.textContent = primaryPack.hint;
      }
    }

    var osChip = document.getElementById("os-chip");
    if (osChip) {
      osChip.classList.remove("is-windows", "is-macos", "is-android");
      if (platform === "windows") {
        osChip.textContent = "У вас Windows";
        osChip.classList.add("is-windows");
      } else if (platform === "macos") {
        osChip.textContent = DOWNLOADS.macos.available ? "У вас Mac" : "У вас Mac · скоро";
        osChip.classList.add("is-macos");
      } else if (platform === "linux") {
        osChip.textContent = DOWNLOADS.linux.available ? "У вас Linux" : "У вас Linux · скоро";
        osChip.classList.add("is-windows");
      } else if (platform === "android") {
        osChip.textContent = DOWNLOADS.android.available ? "У вас Android" : "У вас Android · скоро";
        osChip.classList.add("is-android");
      } else {
        osChip.textContent = "Windows";
        osChip.classList.add("is-windows");
      }
    }

    var versionChip = document.getElementById("version-chip");
    if (versionChip) {
      versionChip.textContent = "v" + primaryPack.version;
    }

    // Platform cards highlight
    $all(document, "[data-platform-card]").forEach(function (card) {
      var id = card.getAttribute("data-platform-card");
      var active = id === primaryPack.id || (id === pack.id && pack.available);
      // Highlight detected OS card even if unavailable
      if (id === pack.id) active = true;
      card.classList.toggle("is-active", active);
    });

    $all(document, "[data-platform-badge]").forEach(function (badge) {
      var id = badge.getAttribute("data-platform-badge");
      var show = id === pack.id;
      badge.hidden = !show;
    });

    updatePlatformPicker(platform);

    // Direct links always point to correct files
    $all(document, "[data-download-link]").forEach(function (link) {
      var id = link.getAttribute("data-download-link");
      var conf = DOWNLOADS[id];
      if (!conf) return;

      if (link.hasAttribute("data-download-format")) return;

      link.setAttribute("href", conf.url);
      if (conf.available) {
        link.setAttribute("download", "");
        link.classList.remove("is-disabled");
        link.removeAttribute("aria-disabled");
      } else {
        link.removeAttribute("download");
        link.classList.add("is-disabled");
        link.setAttribute("aria-disabled", "true");
        link.addEventListener("click", function (e) {
          e.preventDefault();
        });
      }
    });
  }

  function initHeader() {
    var header = document.getElementById("site-header");
    if (!header) return;

    var onScroll = function () {
      header.classList.toggle("is-scrolled", window.scrollY > 8);
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
  }

  function initMobileNav() {
    var toggle = document.querySelector(".nav-toggle");
    var panel = document.getElementById("mobile-nav");
    if (!toggle || !panel) return;

    var reduce =
      window.matchMedia &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    var closing = false;

    var setExpanded = function (open) {
      toggle.setAttribute("aria-expanded", open ? "true" : "false");
      toggle.setAttribute("aria-label", open ? "Закрыть меню" : "Открыть меню");
    };

    var open = function () {
      closing = false;
      panel.hidden = false;
      // next frame so CSS transition runs from 0fr → 1fr
      requestAnimationFrame(function () {
        requestAnimationFrame(function () {
          panel.classList.add("is-open");
          setExpanded(true);
        });
      });
    };

    var close = function () {
      if (panel.hidden && !panel.classList.contains("is-open")) {
        setExpanded(false);
        return;
      }
      if (reduce) {
        panel.classList.remove("is-open");
        panel.hidden = true;
        setExpanded(false);
        return;
      }
      if (closing) return;
      closing = true;
      panel.classList.remove("is-open");
      setExpanded(false);

      var done = false;
      var finish = function () {
        if (done) return;
        done = true;
        panel.hidden = true;
        closing = false;
        panel.removeEventListener("transitionend", onEnd);
      };
      var onEnd = function (e) {
        if (e.target !== panel) return;
        if (e.propertyName !== "grid-template-rows") return;
        finish();
      };
      panel.addEventListener("transitionend", onEnd);
      window.setTimeout(finish, 420);
    };

    toggle.addEventListener("click", function () {
      if (panel.classList.contains("is-open")) close();
      else open();
    });

    $all(panel, "a").forEach(function (a) {
      a.addEventListener("click", close);
    });

    window.addEventListener("resize", function () {
      if (window.innerWidth > 760) close();
    });
  }

  function initReveal() {
    var nodes = $all(document, ".reveal");
    if (!nodes.length) return;

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      nodes.forEach(function (n) {
        n.classList.add("is-visible");
      });
      return;
    }

    if (!("IntersectionObserver" in window)) {
      nodes.forEach(function (n) {
        n.classList.add("is-visible");
      });
      return;
    }

    var io = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            io.unobserve(entry.target);
          }
        });
      },
      { rootMargin: "0px 0px -8% 0px", threshold: 0.12 },
    );

    nodes.forEach(function (n) {
      io.observe(n);
    });
  }

  function initYear() {
    var el = document.getElementById("year");
    if (el) el.textContent = String(new Date().getFullYear());
  }

  /**
   * Smooth open/close for FAQ <details>.
   * Native details jumps shut; we animate height, then toggle [open].
   */
  function initFaq() {
    var items = $all(document, ".faq-item");
    if (!items.length) return;

    var reduce =
      window.matchMedia &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    items.forEach(function (details) {
      var summary = details.querySelector("summary");
      var panel = details.querySelector(".faq-panel");
      if (!summary || !panel) return;

      panel.classList.add("is-js");

      // Sync initial open state to height
      if (details.open) {
        details.classList.add("is-open");
        panel.style.height = "auto";
      } else {
        details.classList.remove("is-open");
        panel.style.height = "0px";
      }

      var busy = false;

      summary.addEventListener("click", function (e) {
        e.preventDefault();
        if (busy) return;

        if (reduce) {
          if (details.open) {
            details.open = false;
            details.classList.remove("is-open");
            panel.style.height = "0px";
          } else {
            details.open = true;
            details.classList.add("is-open");
            panel.style.height = "auto";
          }
          return;
        }

        busy = true;

        if (details.open || details.classList.contains("is-open")) {
          // Close: measure → animate to 0 → remove open
          var startH = panel.scrollHeight;
          panel.style.height = startH + "px";
          // force reflow
          void panel.offsetHeight;
          panel.style.height = "0px";
          details.classList.remove("is-open");

          var onCloseEnd = function (ev) {
            if (ev.propertyName !== "height") return;
            panel.removeEventListener("transitionend", onCloseEnd);
            details.open = false;
            busy = false;
          };
          panel.addEventListener("transitionend", onCloseEnd);
          // Fallback if transitionend missed
          window.setTimeout(function () {
            if (!busy) return;
            details.open = false;
            busy = false;
          }, 450);
        } else {
          // Open: set open → 0 → scrollHeight → auto
          details.open = true;
          details.classList.add("is-open");
          panel.style.height = "0px";
          void panel.offsetHeight;
          var endH = panel.scrollHeight;
          panel.style.height = endH + "px";

          var onOpenEnd = function (ev) {
            if (ev.propertyName !== "height") return;
            panel.removeEventListener("transitionend", onOpenEnd);
            panel.style.height = "auto";
            busy = false;
          };
          panel.addEventListener("transitionend", onOpenEnd);
          window.setTimeout(function () {
            if (!busy) return;
            panel.style.height = "auto";
            busy = false;
          }, 450);
        }
      });
    });
  }

  /**
   * If macOS pack becomes available later, flip config after a HEAD check.
   * Non-blocking; failures keep available:false.
   */
  function probeMacosAvailability() {
    var conf = DOWNLOADS.macos;
    if (!conf || conf.available) return;

    if (typeof fetch !== "function") return;

    fetch(conf.url, { method: "HEAD", cache: "no-store" })
      .then(function (res) {
        if (res && res.ok) {
          conf.available = true;
          applyDownloadUI(detectPlatform());
        }
      })
      .catch(function () {
        /* file not deployed yet — keep fallback */
      });
  }

  /**
   * Обновляет подписи версий из version.php.
   *
   * Числа в разметке — только запасной вариант на случай, если запрос не
   * прошёл: страница закешируется у пользователя, а релиз выйдет позже.
   * Адрес API виден только серверу, браузер ходит на свой же домен.
   */
  function refreshVersions(platform) {
    if (typeof fetch !== "function") return;
    fetch("version.php?_=" + Date.now(), { cache: "no-store", credentials: "same-origin" })
      .then(function (response) {
        return response.ok ? response.json() : null;
      })
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

            pack.debUrl = pack.debAvailable ? fresh.deb_download_url : "";

          }
        });
        $all(document, "[data-version]").forEach(function (node) {
          var fresh = data.platforms[node.getAttribute("data-version")];
          if (fresh && fresh.version) node.textContent = fresh.version;
        });

        $all(document, "[data-download-format='deb'], [data-picker-format='deb']").forEach(function (link) {
          var available = DOWNLOADS.linux.debAvailable === true;
          var cardLink = link.hasAttribute("data-download-format");
          link.hidden = !available && !cardLink;
          link.classList.toggle("is-unavailable", !available);
          if (available) {
            link.href = DOWNLOADS.linux.debUrl;
            link.removeAttribute("aria-disabled");
            link.onclick = null;
          } else {
            link.removeAttribute("href");
            link.setAttribute("aria-disabled", "true");
            link.onclick = function (event) { event.preventDefault(); };
          }
          var status = link.querySelector("[data-deb-status], [data-deb-picker-status]");
          if (status && !available) status.textContent = "Сейчас недоступен";
        });
        applyDownloadUI(platform);
      })
      .catch(function () {
        /* оставляем значения из разметки */
      });
  }

  function init() {
    var platform = detectPlatform();
    applyDownloadUI(platform);
    refreshVersions(platform);
    initHeader();
    initMobileNav();
    initReveal();
    initYear();
    initFaq();
    initPlatformPicker();
    probeMacosAvailability();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
