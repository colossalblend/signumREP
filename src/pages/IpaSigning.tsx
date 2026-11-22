import { open } from "@tauri-apps/plugin-dialog";
import {
  Check, CheckCircle2, ChevronDown, Clock, FileArchive, FolderOpen, LoaderCircle,
  Package, RefreshCw, Save, Settings2, ShieldCheck, Smartphone, Trash2, Upload, X, Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import Checkbox from "../components/Checkbox";
import PageHeader from "../components/PageHeader";
import { command, errorMessage } from "../lib/tauri";
import { tourIpaDecrypted, useTourDemo } from "../lib/tourStore";
import type { AuthStatus, DeviceInfo, IpaInspect, SavedCertInfo, SignedAppRecord, SignOptions } from "../types";

const defaultOptions = (): SignOptions => ({
  lower_min_ios: false,
  min_ios_version: "12.0",
  file_sharing: false,
  remove_device_limits: false,
  change_app_name: false,
  app_name: "",
  change_version: false,
  app_version: "",
  auto_bundle_id: true,
  custom_bundle_id: "",
  force_unique_bundle: false,
  strip_plugins: false,
  strip_watch: true,
  clear_url_schemes: false,
  documents_in_place: false,
  supports_opening_docs: false,
  allow_http: false,
  remove_orientation_lock: false,
  strip_frameworks: false,
});

export default function IpaSigning() {
  const navigate = useNavigate();
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [udid, setUdid] = useState("");
  const [ipaPath, setIpaPath] = useState("");
  const [inspect, setInspect] = useState<IpaInspect | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [install, setInstall] = useState(true);
  const [saveSigned, setSaveSigned] = useState(false);
  const [outputDir, setOutputDir] = useState("");
  const [options, setOptions] = useState<SignOptions>(defaultOptions);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function renewSignature(item: SignedAppRecord) {
    if (renewing) return;
    setRenewing(item.bundle_id);
    setError("");
    try {
      await command<string>("renew_signed_app", { input: { bundleId: item.bundle_id } });
      navigate("/installs");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRenewing("");
    }
  }

  const loadSignedApps = useCallback(async () => {
    try {
      setSignedApps(await command<SignedAppRecord[]>("list_signed_apps"));
    } catch {

    }
  }, []);

  const [certMode, setCertMode] = useState<"account" | "custom">("account");
  const [savedCerts, setSavedCerts] = useState<SavedCertInfo[]>([]);
  const [certName, setCertName] = useState("");
  const [customP12, setCustomP12] = useState("");
  const [customProv, setCustomProv] = useState("");
  const [certPassword, setCertPassword] = useState("");
  const [signedApps, setSignedApps] = useState<SignedAppRecord[]>([]);
  const [renewing, setRenewing] = useState("");
  const [certBusy, setCertBusy] = useState(false);
  const [certError, setCertError] = useState("");

  const setOpt = useCallback(<K extends keyof SignOptions>(key: K, value: SignOptions[K]) => {
    setOptions(prev => ({ ...prev, [key]: value }));
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [status, list, settings] = await Promise.all([
        command<AuthStatus>("get_auth_status"),
        // No iPhone is fine here: signing can still export the IPA.
        command<DeviceInfo[]>("list_connected_devices").catch(() => [] as DeviceInfo[]),
        command<{ selected_device_udid?: string; exports_dir?: string }>("get_settings"),
      ]);
      setAuth(status);
      setDevices(list);
      setUdid(current => {
        if (current && list.some(d => d.udid === current)) return current;
        const saved = settings.selected_device_udid?.trim();
        if (saved && list.some(d => d.udid === saved)) return saved;
        return list[0]?.udid || "";
      });
      if (!outputDir && settings.exports_dir) setOutputDir(settings.exports_dir);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [outputDir]);

  useEffect(() => { void refresh(); }, [refresh]);

  useEffect(() => {
    command<SavedCertInfo[]>("list_custom_certs").then(setSavedCerts).catch(() => setSavedCerts([]));
  }, [certBusy]);

  useEffect(() => { void loadSignedApps(); }, [loadSignedApps]);

  // The walkthrough runs on demo data so the file card has something real to
  // show without the user having to find an IPA first.
  const tourDemo = useTourDemo();
  const shownInspect = tourDemo ? tourIpaDecrypted : inspect;
  const needsSign = tourDemo ? true : inspect?.needs_sign === true;
  const chosenSavedCert = savedCerts.find(c => c.name === certName.trim());
  const customPathsReady = !!customP12.trim() && !!customProv.trim();
  const signingReady = certMode === "custom"
    ? (!!chosenSavedCert || customPathsReady)
    : (!!auth?.authenticated || (!!auth?.email && !!auth?.historyPasswordSaved));
  const ready = useMemo(() => {
    if (!ipaPath || !inspect || inspect.kind === "invalid") return false;
    if (needsSign) {
      if (!signingReady) return false;
      if (install && !udid) return false;
      if (!install && !saveSigned) return false;
      if (options.change_app_name && !options.app_name?.trim()) return false;
      if (options.change_version && !options.app_version?.trim()) return false;
      if (!options.auto_bundle_id && !options.custom_bundle_id?.trim()) return false;
      return true;
    }
    return !!udid;
  }, [ipaPath, inspect, needsSign, signingReady, install, udid, saveSigned, options]);

  async function chooseIpa() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "IPA", extensions: ["ipa"] }],
    });
    if (typeof path === "string") {
      setIpaPath(path);
      setInspect(null);
      setError("");
      setInspecting(true);
      try {
        const report = await command<IpaInspect>("inspect_ipa", { path });
        setInspect(report);
      } catch (e) {
        setError(errorMessage(e));
      } finally {
        setInspecting(false);
      }
    }
  }

  async function chooseOutput() {
    const path = await open({ multiple: false, directory: true });
    if (typeof path === "string") setOutputDir(path);
  }

  function buildOptionsPayload(): SignOptions {
    return {
      lower_min_ios: !!options.lower_min_ios,
      min_ios_version: options.lower_min_ios
        ? (options.min_ios_version?.trim() || "12.0")
        : null,
      file_sharing: !!options.file_sharing,
      remove_device_limits: !!options.remove_device_limits,
      change_app_name: !!options.change_app_name,
      app_name: options.change_app_name ? (options.app_name?.trim() || null) : null,
      change_version: !!options.change_version,
      app_version: options.change_version ? (options.app_version?.trim() || null) : null,
      auto_bundle_id: options.auto_bundle_id !== false,
      custom_bundle_id: options.auto_bundle_id === false
        ? (options.custom_bundle_id?.trim() || null)
        : null,
      force_unique_bundle: !!options.force_unique_bundle,
      strip_plugins: !!options.strip_plugins,
      strip_watch: !!options.strip_watch,
      clear_url_schemes: !!options.clear_url_schemes,
      documents_in_place: !!options.file_sharing || !!options.documents_in_place,
      supports_opening_docs: !!options.file_sharing || !!options.supports_opening_docs,
      allow_http: !!options.allow_http,
      remove_orientation_lock: !!options.remove_orientation_lock,
      strip_frameworks: !!options.strip_frameworks,
    };
  }

  async function startSign() {
    setBusy(true);
    setError("");
    try {
      if (udid) {
        await command("select_device", { input: { udid } });
      }
      if (needsSign) {
        await command<string>("enqueue_sign_and_install", {
          input: {
            ipa_path: ipaPath,
            udid: udid || null,
            install,
            save_signed: saveSigned,
            output_dir: saveSigned ? (outputDir || null) : null,
            cert_mode: certMode === "custom" ? "custom" : "auto",
            cert_name: certMode === "custom" && chosenSavedCert ? chosenSavedCert.name : null,
            custom_p12_path: certMode === "custom" && !chosenSavedCert ? customP12 : null,
            custom_prov_path: certMode === "custom" && !chosenSavedCert ? customProv : null,
            custom_cert_password: certMode === "custom" && !chosenSavedCert && certPassword ? certPassword : null,
            options: buildOptionsPayload(),
          },
        });
      } else {
        await command<string>("enqueue_local_install", { path: ipaPath });
      }
      navigate("/installs");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const ipaName = ipaPath ? ipaPath.replace(/^.*[\\/]/, "") : "";

  return (
    <div className="mx-auto max-w-6xl p-7 lg:p-9">
      <PageHeader
        title="IPA-файл"
        subtitle="Выберите файл на компьютере. Signum покажет, что с ним делать."
        actions={
          <button type="button" className="btn-secondary" onClick={() => void refresh()} disabled={loading || busy}>
            <RefreshCw size={15} className={loading ? "animate-spin" : undefined} />
            Обновить
          </button>
        }
      />

      {signedApps.length > 0 && (
        <section className="panel mt-8 p-5">
          <div className="flex items-center gap-2 font-bold">
            <Clock size={18} className="text-brand-300" />
            Подписано через Signum
          </div>
          <p className="mt-1.5 text-[11px] leading-5 text-slate-500">
            Подпись бесплатного Apple ID живёт неделю. Когда срок выйдет, приложение
            перестанет открываться — подпишите заново той же кнопкой.
          </p>
          <div className="mt-4 space-y-2">
            {signedApps.map(item => {
              const left = signatureLeft(item);
              return (
                <div key={item.bundle_id} className="flex items-center justify-between gap-4 rounded-xl border border-white/[0.06] bg-black/10 p-3.5">
                  <div className="min-w-0">
                    <div className="truncate text-xs font-semibold text-theme">{item.title || item.bundle_id}</div>
                    <div className={`mt-0.5 text-[11px] ${left?.soon ? "text-amber-300" : "text-slate-500"}`}>
                      {left?.text ?? "Срок подписи неизвестен"}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="btn-secondary shrink-0 px-2.5 py-1.5 text-[11px]"
                    disabled={renewing === item.bundle_id || busy}
                    onClick={() => void renewSignature(item)}
                  >
                    {renewing === item.bundle_id ? <LoaderCircle size={13} className="animate-spin" /> : null}
                    Подписать заново
                  </button>
                </div>
              );
            })}
          </div>
        </section>
      )}

      <section data-tour="ipa-file" className="panel mt-8 p-5">
        <div className="flex items-center gap-2 font-bold">
          <FileArchive size={18} className="text-brand-300" />
          Файл
        </div>
        <div className="mt-4 flex flex-wrap items-center gap-3">
          <button type="button" className="btn-secondary" onClick={() => void chooseIpa()}>
            <Upload size={16} />
            Выбрать файл
          </button>
          {ipaPath && (
            <button type="button" className="btn-secondary px-3" onClick={() => { setIpaPath(""); setInspect(null); }} title="Очистить">
              <X size={15} />
            </button>
          )}
        </div>
        {ipaName && (
          <p className="mt-3 break-all text-xs text-slate-400">{ipaName}</p>
        )}
        {inspecting && (
          <div className="mt-4 flex items-center gap-2 text-sm text-slate-500">
            <LoaderCircle className="animate-spin text-brand-400" size={16} />
            Смотрим файл…
          </div>
        )}
        {shownInspect && (
          <div className={`mt-4 rounded-xl border px-4 py-3 ${
            shownInspect.kind === "invalid"
              ? "border-amber-400/25 bg-amber-400/[0.06]"
              : shownInspect.kind === "signed"
                ? "border-emerald-400/25 bg-emerald-400/[0.07]"
                : shownInspect.kind === "encrypted"
                  ? "border-sky-400/25 bg-sky-400/[0.07]"
                  : "border-brand-400/25 bg-brand-500/[0.08]"
          }`}>
            <div className="text-sm font-bold text-theme">{shownInspect.title}</div>
            {(shownInspect.app_name || shownInspect.version) && (
              <div className="mt-1 text-xs text-muted">
                {[shownInspect.app_name, shownInspect.version].filter(Boolean).join(" · ")}
              </div>
            )}
            <p className="mt-2 text-sm leading-5 text-slate-300">{shownInspect.explanation}</p>
            <dl className="mt-3 grid gap-2 text-xs text-muted sm:grid-cols-2">
              {shownInspect.bundle_id && <div><dt>Bundle ID</dt><dd className="break-all text-theme">{shownInspect.bundle_id}</dd></div>}
              {shownInspect.min_ios && <div><dt>Минимальная iOS</dt><dd className="text-theme">{shownInspect.min_ios}</dd></div>}
              {!!shownInspect.architectures?.length && <div><dt>Архитектуры</dt><dd className="text-theme">{shownInspect.architectures.join(", ")}</dd></div>}
              {shownInspect.profile_expires_at && <div><dt>Профиль действует до</dt><dd className="text-theme">{new Date(shownInspect.profile_expires_at).toLocaleString("ru-RU")}</dd></div>}
            </dl>
            {!!shownInspect.extensions?.length && <p className="mt-3 break-all text-xs text-muted">Расширения: {shownInspect.extensions.join(", ")}</p>}
            {shownInspect.warnings?.map(warning => <p key={warning} className="mt-2 text-xs leading-5 text-amber-200">{warning}</p>)}
          </div>
        )}
      </section>

      {needsSign && (
      <section data-tour="ipa-cert" className="panel mt-5 p-5">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 font-bold">
            <ShieldCheck size={18} className={signingReady ? "text-emerald-400" : "text-amber-400"} />
            Способ подписи
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setCertMode("account")}
              className={`rounded-xl border px-3 py-1.5 text-xs font-semibold transition ${
                certMode === "account"
                  ? "border-brand-400/40 bg-brand-500/15 text-brand-200"
                  : "border-white/10 bg-black/10 text-slate-400 hover:border-white/20"
              }`}
            >
              Через Apple ID (7 дней)
            </button>
            <button
              type="button"
              onClick={() => setCertMode("custom")}
              className={`rounded-xl border px-3 py-1.5 text-xs font-semibold transition ${
                certMode === "custom"
                  ? "border-brand-400/40 bg-brand-500/15 text-brand-200"
                  : "border-white/10 bg-black/10 text-slate-400 hover:border-white/20"
              }`}
            >
              Своим сертификатом (до года)
            </button>
          </div>
        </div>

        {certMode === "account" ? (
          <p className="mt-3 text-sm text-slate-500">
            {signingReady
              ? (auth?.email ?? "Apple ID")
              : "Войдите в Apple ID на странице «Аккаунт» — без него файл не подписать."}
          </p>
        ) : (
          <div className="mt-4 space-y-3">
            {savedCerts.length > 0 && (
              <div className="space-y-2">
                <div className="text-xs font-semibold text-slate-400">Сохранённые сертификаты</div>
                {savedCerts.map(cert => {
                  const selected = certName.trim() === cert.name;
                  return (
                    <div
                      key={cert.name}
                      className={`flex items-center gap-3 rounded-xl border px-3 py-2.5 text-left transition ${
                        selected
                          ? "border-brand-400/35 bg-brand-500/10"
                          : "border-white/[0.08] bg-black/10 hover:border-white/15"
                      }`}
                    >
                      <button
                        type="button"
                        className="flex-1 text-left"
                        onClick={() => {
                          setCertName(cert.name);
                          setCustomP12("");
                          setCustomProv("");
                        }}
                      >
                        <div className="text-xs font-semibold text-theme">{cert.name}</div>
                        <div className="mt-0.5 text-[11px] text-slate-500">
                          {cert.has_password ? "Пароль сохранён" : "Пароль не сохранён"} · готов к подписи
                        </div>
                        {certValidity(cert.expires_at) && (
                          <div className={`mt-0.5 text-[11px] ${certValidity(cert.expires_at)!.soon ? "text-amber-300" : "text-slate-600"}`}>
                            {certValidity(cert.expires_at)!.text}
                          </div>
                        )}
                      </button>
                      <button
                        type="button"
                        className="btn-secondary px-2.5 py-1.5 text-[11px]"
                        title="Удалить сохранённый сертификат"
                        onClick={() => {
                          if (!window.confirm(`Удалить сертификат «${cert.name}»?`)) return;
                          void (async () => {
                            setCertBusy(true);
                            try {
                              await command("delete_custom_cert", { name: cert.name });
                              if (certName.trim() === cert.name) setCertName("");
                            } catch (e) {
                              setCertError(errorMessage(e));
                            } finally {
                              setCertBusy(false);
                            }
                          })();
                        }}
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  );
                })}
              </div>
            )}

            {!chosenSavedCert && (
              <div className="space-y-2.5 rounded-xl border border-white/[0.07] bg-black/10 p-3">
                <div className="text-xs font-semibold text-slate-400">Файлы сертификата</div>
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    className="btn-secondary text-xs"
                    onClick={() => void (async () => {
                      const path = await open({
                        multiple: false,
                        filters: [{ name: "Сертификат", extensions: ["p12"] }],
                      });
                      if (typeof path === "string") setCustomP12(path);
                    })()}
                  >
                    <Upload size={14} />
                    Файл .p12
                  </button>
                  {customP12 && <span className="break-all text-[11px] text-slate-500">{customP12.replace(/^.*[\\/]/, "")}</span>}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    className="btn-secondary text-xs"
                    onClick={() => void (async () => {
                      const path = await open({
                        multiple: false,
                        filters: [{ name: "Профиль", extensions: ["mobileprovision"] }],
                      });
                      if (typeof path === "string") setCustomProv(path);
                    })()}
                  >
                    <Upload size={14} />
                    Файл .mobileprovision
                  </button>
                  {customProv && <span className="break-all text-[11px] text-slate-500">{customProv.replace(/^.*[\\/]/, "")}</span>}
                </div>
                <input
                  className="field text-xs"
                  type="password"
                  placeholder="Пароль от .p12 (если есть)"
                  value={certPassword}
                  onChange={e => setCertPassword(e.target.value)}
                />
                {customPathsReady && (
                  <div className="flex flex-wrap items-center gap-2 border-t border-white/[0.06] pt-2.5">
                    <input
                      className="field max-w-[200px] py-1.5 text-xs"
                      placeholder="Имя для сохранения, напр. «Рабочий 2026»"
                      value={certName}
                      onChange={e => setCertName(e.target.value)}
                    />
                    <button
                      type="button"
                      className="btn-secondary text-xs"
                      disabled={certBusy || !certName.trim()}
                      onClick={() => void (async () => {
                        if (!certName.trim()) return;
                        setCertBusy(true);
                        setCertError("");
                        try {
                          // ImportCustomCertInput has no rename_all, so the
                          // struct wants snake_case inside the `input` wrapper.
                          await command("import_custom_cert", {
                            input: {
                              name: certName.trim(),
                              p12_path: customP12,
                              prov_path: customProv,
                              password: certPassword.trim() || null,
                            },
                          });
                          setCustomP12("");
                          setCustomProv("");
                          setCertPassword("");
                        } catch (e) {
                          setCertError(errorMessage(e));
                        } finally {
                          setCertBusy(false);
                        }
                      })()}
                    >
                      <Save size={13} />
                      Сохранить
                    </button>
                  </div>
                )}
              </div>
            )}
            {certError && (
              <p className="text-xs text-amber-300">{certError}</p>
            )}
            <p className="text-[11px] leading-4 text-slate-500">
              Apple ID не нужен. Файлы лежат в папке приложения, пароль — в хранилище Windows.
            </p>
          </div>
        )}
        {certMode === "account" && !signingReady && (
          <button type="button" className="btn-primary mt-3" onClick={() => navigate("/account")}>
            Войти
          </button>
        )}
      </section>
      )}


      {                   }
      <section className="panel mt-5 p-5">
        <div className="flex items-center gap-2 font-bold">
          <Smartphone size={18} className="text-brand-300" />
          Устройство
        </div>
        {loading ? (
          <div className="mt-4 flex items-center gap-2 text-sm text-slate-500">
            <LoaderCircle className="animate-spin text-brand-400" size={18} />
            Ищем iPhone…
          </div>
        ) : devices.length === 0 ? (
          <div className="mt-4 rounded-xl border border-dashed border-white/10 bg-black/10 px-4 py-6 text-center">
            <Smartphone size={28} className="mx-auto text-slate-600" />
            <p className="mt-3 text-sm text-slate-500">
              {needsSign
                ? "iPhone не подключён. Чтобы сохранить файл после подписи, телефон не нужен. Чтобы сразу установить — подключите кабель."
                : "Подключите iPhone кабелем, чтобы поставить этот файл."}
            </p>
          </div>
        ) : (
          <div className={`mt-4 grid gap-2.5 ${devices.length > 1 ? "sm:grid-cols-2" : ""}`}>
            {devices.map(device => {
              const selected = udid === device.udid;
              const name = device.device_name ?? device.product_type ?? "iPhone";
              const ios = device.product_version ? `iOS ${device.product_version}` : "iOS —";
              const model = device.product_type ?? "iPhone";
              return (
                <button
                  key={device.udid}
                  type="button"
                  onClick={() => setUdid(device.udid)}
                  className={`group relative flex items-center gap-3.5 rounded-2xl border p-4 text-left transition ${
                    selected
                      ? "border-brand-400/40 bg-brand-500/10 shadow-[0_0_0_1px_rgba(91,108,240,0.12)]"
                      : "border-white/[0.07] bg-black/10 hover:border-white/15 hover:bg-white/[0.03]"
                  }`}
                >
                  <div
                    className={`grid h-12 w-12 shrink-0 place-items-center rounded-2xl border transition ${
                      selected
                        ? "border-brand-400/30 bg-brand-500/20 text-brand-200"
                        : "border-white/10 bg-white/[0.04] text-slate-400 group-hover:text-slate-300"
                    }`}
                  >
                    <Smartphone size={22} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm font-semibold text-theme">{name}</span>
                      {selected && (
                        <span className="shrink-0 rounded-full bg-brand-500/20 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-brand-200">
                          Выбрано
                        </span>
                      )}
                    </div>
                    <div className="mt-1 truncate text-xs text-slate-500">
                      {ios}
                      <span className="mx-1.5 text-slate-600">·</span>
                      {model}
                    </div>
                  </div>
                  <span
                    className={`grid h-6 w-6 shrink-0 place-items-center rounded-full border transition ${
                      selected
                        ? "border-brand-400/50 bg-brand-500 text-white shadow-[0_0_12px_rgba(91,108,240,0.45)]"
                        : "border-white/15 bg-transparent text-transparent"
                    }`}
                    aria-hidden
                  >
                    <Check size={14} strokeWidth={2.5} />
                  </span>
                </button>
              );
            })}
          </div>
        )}
      </section>

      {needsSign && <section className="panel mt-5 space-y-2.5 p-5">
        <div className="mb-1 text-sm font-bold">Действия</div>
        <Checkbox
          checked={install}
          onChange={setInstall}
          label="Установить на iPhone"
          description={needsSign
            ? "После подписи сразу поставить на выбранный iPhone. Нужен кабель."
            : "Поставить выбранный файл на iPhone. Нужен кабель."}
        />
        {needsSign && <Checkbox
          checked={saveSigned}
          onChange={setSaveSigned}
          label="Сохранить готовый файл"
          description="Оставить копию на компьютере. Телефон не обязателен."
        />}
        {saveSigned && (
          <div className="flex flex-wrap items-center gap-3 rounded-xl border border-white/[0.06] bg-black/10 px-3 py-3">
            <button type="button" className="btn-secondary" onClick={() => void chooseOutput()}>
              <FolderOpen size={15} />
              Папка
            </button>
            {outputDir ? (
              <span className="min-w-0 flex-1 break-all text-xs text-slate-400">{outputDir}</span>
            ) : (
              <span className="text-xs text-slate-500">Папка по умолчанию (экспорт)</span>
            )}
          </div>
        )}
      </section>}

      {needsSign && <section className="panel mt-5 p-5">
        <button
          type="button"
          className="flex w-full items-center justify-between gap-3 text-left"
          onClick={() => setShowAdvanced(v => !v)}
        >
          <div className="flex items-center gap-2 font-bold">
            <Settings2 size={18} className="text-brand-300" />
            Опции подписи
          </div>
          <ChevronDown
            size={18}
            className={`shrink-0 text-slate-500 transition ${showAdvanced ? "rotate-180" : ""}`}
          />
        </button>
        <p className="mt-1.5 text-xs text-slate-500">
          Имя, совместимость, доступ к файлам и другие правки перед подписью.
        </p>

        {showAdvanced && (
          <div className="mt-5 space-y-5">
            {              }
            <div>
              <div className="mb-2.5 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
                <Package size={13} />
                Идентификация
              </div>
              <div className="space-y-2.5">
                <Checkbox
                  checked={!!options.change_app_name}
                  onChange={v => setOpt("change_app_name", v)}
                  label="Изменить имя приложения"
                  description="Имя на домашнем экране iPhone"
                />
                {options.change_app_name && (
                  <input
                    className="field"
                    placeholder="Новое имя"
                    value={options.app_name ?? ""}
                    onChange={e => setOpt("app_name", e.target.value)}
                  />
                )}

                <Checkbox
                  checked={!!options.change_version}
                  onChange={v => setOpt("change_version", v)}
                  label="Изменить версию"
                  description="Номер версии приложения"
                />
                {options.change_version && (
                  <input
                    className="field"
                    placeholder="1.0.0"
                    value={options.app_version ?? ""}
                    onChange={e => setOpt("app_version", e.target.value)}
                  />
                )}

                <Checkbox
                  checked={options.auto_bundle_id !== false}
                  onChange={v => setOpt("auto_bundle_id", v)}
                  label="Автоматический идентификатор"
                  description="Оставить оригинальный идентификатор (команда добавит суффикс)"
                />
                {options.auto_bundle_id === false && (
                  <input
                    className="field font-mono text-xs"
                    placeholder="com.example.myapp"
                    value={options.custom_bundle_id ?? ""}
                    onChange={e => setOpt("custom_bundle_id", e.target.value)}
                  />
                )}

                <Checkbox
                  checked={!!options.force_unique_bundle}
                  onChange={v => setOpt("force_unique_bundle", v)}
                  label="Уникальный идентификатор"
                  description="Случайный суффикс — можно ставить рядом со старой установкой"
                />
              </div>
            </div>

            {                   }
            <div>
              <div className="mb-2.5 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
                <Smartphone size={13} />
                Совместимость
              </div>
              <div className="space-y-2.5">
                <Checkbox
                  checked={!!options.lower_min_ios}
                  onChange={v => setOpt("lower_min_ios", v)}
                  label="Поддержка старых iOS"
                  description="Понизить MinimumOSVersion (как «Install older apps»)"
                />
                {options.lower_min_ios && (
                  <div className="flex flex-wrap items-center gap-2">
                    {["12.0", "13.0", "14.0", "15.0", "16.0"].map(v => (
                      <button
                        key={v}
                        type="button"
                        onClick={() => setOpt("min_ios_version", v)}
                        className={`rounded-lg border px-3 py-1.5 text-xs font-semibold transition ${
                          (options.min_ios_version || "12.0") === v
                            ? "border-brand-400/40 bg-brand-500/15 text-brand-200"
                            : "border-white/10 bg-black/10 text-slate-400 hover:border-white/20"
                        }`}
                      >
                        iOS {v}
                      </button>
                    ))}
                    <input
                      className="field max-w-[7rem] py-1.5 text-xs"
                      placeholder="12.0"
                      value={options.min_ios_version ?? ""}
                      onChange={e => setOpt("min_ios_version", e.target.value)}
                    />
                  </div>
                )}

                <Checkbox
                  checked={!!options.remove_device_limits}
                  onChange={v => setOpt("remove_device_limits", v)}
                  label="Снять ограничения устройств"
                  description="Убрать UIRequiredDeviceCapabilities, iPhone + iPad"
                />

                <Checkbox
                  checked={!!options.remove_orientation_lock}
                  onChange={v => setOpt("remove_orientation_lock", v)}
                  label="Снять lock ориентации"
                  description="Убрать жёсткие UISupportedInterfaceOrientations"
                />
              </div>
            </div>

            {              }
            <div>
              <div className="mb-2.5 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
                <Zap size={13} />
                Функции
              </div>
              <div className="space-y-2.5">
                <Checkbox
                  checked={!!options.file_sharing}
                  onChange={v => setOpt("file_sharing", v)}
                  label="File Sharing"
                  description="Доступ к файлам приложения через Finder / Файлы"
                />

                <Checkbox
                  checked={!!options.strip_plugins}
                  onChange={v => setOpt("strip_plugins", v)}
                  label="Удалить плагины / extensions"
                  description="Меньше App ID — полезно для free Apple ID (лимит 10)"
                />

                <Checkbox
                  checked={!!options.strip_watch}
                  onChange={v => setOpt("strip_watch", v)}
                  label="Удалить Apple Watch"
                  description="Убрать Watch / WatchKit companion"
                />

                <Checkbox
                  checked={!!options.clear_url_schemes}
                  onChange={v => setOpt("clear_url_schemes", v)}
                  label="Отключить открытие по ссылкам"
                  description="Чтобы ссылки не открывались в копии вместо оригинального приложения"
                />

                <Checkbox
                  checked={!!options.allow_http}
                  onChange={v => setOpt("allow_http", v)}
                  label="Разрешить HTTP"
                  description="Разрешает подключения без шифрования. Включайте, только если это необходимо для работы приложения."
                />

                <Checkbox
                  checked={!!options.strip_frameworks}
                  onChange={v => setOpt("strip_frameworks", v)}
                  label="Удалить вспомогательные библиотеки"
                  description="Удаляет библиотеки виджетов, расширений и Apple Watch. Приложение может перестать работать."
                />
              </div>
            </div>

            <button
              type="button"
              className="btn-secondary text-xs"
              onClick={() => setOptions(defaultOptions())}
            >
              Сбросить опции
            </button>
          </div>
        )}
      </section>}

      <div className="mt-5 flex flex-wrap items-center gap-3">
        <button
          type="button"
          className="btn-primary"
          disabled={!ready || busy || loading || inspecting}
          onClick={() => void startSign()}
        >
          {busy ? <LoaderCircle size={16} className="animate-spin" /> : <CheckCircle2 size={16} />}
          {busy
            ? "В очередь…"
            : needsSign
              ? (install ? "Подписать и установить" : "Подписать")
              : "Установить"}
        </button>
        {needsSign && !install && !saveSigned && (
          <span className="text-xs text-amber-300/90">Выберите установку или сохранение файла</span>
        )}
        {!needsSign && inspect && inspect.kind !== "invalid" && !udid && (
          <span className="text-xs text-amber-300/90">Подключите iPhone, чтобы поставить файл</span>
        )}
      </div>

      {error && (
        <div className="mt-5 rounded-xl border border-amber-400/25 bg-amber-400/[0.06] p-4 text-sm text-slate-300">
          {error}
        </div>
      )}
    </div>
  );
}

/// Сколько ещё проживёт профиль подписи. У бесплатной учётки это неделя,
/// поэтому «истекает» надо показывать заранее, а не в день отказа.
function certValidity(expiresAt?: string | null): { text: string; soon: boolean } | null {
  const raw = expiresAt?.trim();
  if (!raw) return null;
  const until = new Date(raw);
  if (Number.isNaN(until.getTime())) return null;
  const days = Math.floor((until.getTime() - Date.now()) / 86_400_000);
  const date = until.toLocaleDateString("ru-RU");
  if (days < 0) return { text: `Профиль истёк ${date} — подписать им не получится`, soon: true };
  if (days === 0) return { text: "Профиль истекает сегодня", soon: true };
  if (days <= 3) return { text: `Профиль истекает через ${days} дн. (${date})`, soon: true };
  return { text: `Действует до ${date}`, soon: false };
}

/// Сколько осталось подписи. Бесплатный профиль живёт неделю от подписания,
/// поэтому при неизвестном сроке считаем от даты подписи.
function signatureLeft(item: SignedAppRecord): { text: string; soon: boolean } | null {
  const until = item.expires_unix
    ? item.expires_unix * 1000
    : item.team_kind === "free"
      ? (item.signed_at + 7 * 86_400) * 1000
      : null;
  if (!until) return null;
  const days = Math.floor((until - Date.now()) / 86_400_000);
  const date = new Date(until).toLocaleDateString("ru-RU");
  if (days < 0) return { text: `Подпись истекла ${date} — приложение не откроется`, soon: true };
  if (days === 0) return { text: "Подпись истекает сегодня", soon: true };
  if (days <= 2) return { text: `Подпись истекает через ${days} дн. (${date})`, soon: true };
  return { text: `Подпись действует до ${date}`, soon: false };
}
