   
                                                                  
                                           
   
(function () {
  "use strict";

                                                                

  var DOWNLOADS = {
    windows: {
      id: "windows",
      label: "Скачать для Windows",
      shortLabel: "Windows",
      hint: "Windows 10/11 · версия 1.1.1",
      url: "downloads/windows/Signum_1.1.1_x64-setup.exe",
      version: "1.1.1",
      available: true,
    },
    macos: {
      id: "macos",
      label: "Скачать для Mac",
      shortLabel: "Mac",
      hint: "Mac · версия 1.1.1",
      url: "downloads/macos/Signum.dmg",
      version: "1.1.1",
                                                              
      available: false,
    },
    linux: {
      id: "linux",
      label: "Скачать для Linux",
      shortLabel: "Linux",
      hint: "Linux · AppImage · версия 1.1.1",
      url: "downloads/linux/Signum.AppImage",
      version: "1.1.1",
                                                                   
      available: false,
    },
  };

  var FALLBACK = DOWNLOADS.windows;

     
                                              
                                                                       
                                                                               
                        
     
  function detectPlatform() {
    try {
      var uaData = navigator.userAgentData;
      if (uaData && typeof uaData.platform === "string" && uaData.platform) {
        var p = uaData.platform.toLowerCase();
        if (p.indexOf("win") !== -1) return "windows";
        if (p.indexOf("mac") !== -1) return "macos";
        if (p.indexOf("linux") !== -1 || p.indexOf("chrome os") !== -1) return "linux";
      }
    } catch (_) {
                  
    }

    var platform = String(navigator.platform || "").toLowerCase();
    var ua = String(navigator.userAgent || "").toLowerCase();

                                                                       
    var isIpad = /ipad/.test(ua) || (platform === "macintel" && navigator.maxTouchPoints > 1);
    var isIphone = /iphone|ipod/.test(ua);
    if (isIpad || isIphone) return "macos";

    if (/win/.test(platform) || /windows/.test(ua)) return "windows";
    if (/mac/.test(platform) || /macintosh|mac os x/.test(ua)) return "macos";
    if (/linux/.test(platform) || /linux|cros/.test(ua)) return "linux";

    return "unknown";
  }

     
                               
     
  function resolveDownload(platform) {
    if (platform === "macos") return DOWNLOADS.macos;
    if (platform === "windows") return DOWNLOADS.windows;
    if (platform === "linux") return DOWNLOADS.linux;
    return DOWNLOADS.windows;
  }

     
                                       
                             
     
  function $all(root, selector) {
    return Array.prototype.slice.call(root.querySelectorAll(selector));
  }

  function applyDownloadUI(platform) {
    var pack = resolveDownload(platform);
    var primaryPack = pack.available ? pack : FALLBACK;

                   
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
      if (platform === "macos" && !DOWNLOADS.macos.available) {
        hint.textContent = "Для Mac скоро. Пока можно Windows · v" + FALLBACK.version;
      } else if (platform === "linux" && !DOWNLOADS.linux.available) {
        hint.textContent = "Для Linux скоро. Пока можно Windows · v" + FALLBACK.version;
      } else {
        hint.textContent = primaryPack.hint;
      }
    }

    var osChip = document.getElementById("os-chip");
    if (osChip) {
      osChip.classList.remove("is-windows", "is-macos");
      if (platform === "windows") {
        osChip.textContent = "У вас Windows";
        osChip.classList.add("is-windows");
      } else if (platform === "macos") {
        osChip.textContent = DOWNLOADS.macos.available ? "У вас Mac" : "У вас Mac · скоро";
        osChip.classList.add("is-macos");
      } else if (platform === "linux") {
        osChip.textContent = DOWNLOADS.linux.available ? "У вас Linux" : "У вас Linux · скоро";
        osChip.classList.add("is-windows");
      } else {
        osChip.textContent = "Windows";
        osChip.classList.add("is-windows");
      }
    }

    var versionChip = document.getElementById("version-chip");
    if (versionChip) {
      versionChip.textContent = "v" + primaryPack.version;
    }

                               
    $all(document, "[data-platform-card]").forEach(function (card) {
      var id = card.getAttribute("data-platform-card");
      var active = id === primaryPack.id || (id === pack.id && pack.available);
                                                       
      if (id === pack.id) active = true;
      card.classList.toggle("is-active", active);
    });

    $all(document, "[data-platform-badge]").forEach(function (badge) {
      var id = badge.getAttribute("data-platform-badge");
      var show = id === pack.id;
      badge.hidden = !show;
    });

                            
    $all(document, "[data-macos-note]").forEach(function (note) {
      note.hidden = DOWNLOADS.macos.available;
    });
    $all(document, "[data-linux-note]").forEach(function (note) {
      note.hidden = DOWNLOADS.linux.available;
    });

                                                 
    $all(document, "[data-download-link]").forEach(function (link) {
      var id = link.getAttribute("data-download-link");
      var conf = DOWNLOADS[id];
      if (!conf) return;
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
                                                        
          var startH = panel.scrollHeight;
          panel.style.height = startH + "px";
                         
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
                                             
          window.setTimeout(function () {
            if (!busy) return;
            details.open = false;
            busy = false;
          }, 450);
        } else {
                                                     
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
                                                   
      });
  }

  function init() {
    var platform = detectPlatform();
    applyDownloadUI(platform);
    initHeader();
    initMobileNav();
    initReveal();
    initYear();
    initFaq();
    probeMacosAvailability();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
