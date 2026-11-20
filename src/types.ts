export type Badge = "store" | "restore" | "soon" | string;

export interface IcloudEnsureResult {
  status: "ready" | "not_needed" | "installed" | "failed" | "missing" | string;
  installed: boolean;
  message?: string | null;
}

export interface IcloudSetupProgress {
  phase: string;
  percent?: number;
  message?: string;
}

                                                                                  
export type ActivationMode = "deeplink" | "in_app_scan" | string;

export interface AppActivationItem {
  id: string;
  title: string;
  subtitle?: string;
                                                                  
  url?: string;
                                                                          
  qr_payload?: string;
  mode?: ActivationMode;
  hint?: string;
  app_store_id?: string;
  bundle_id?: string;
                                                         
  tb_id?: string;
}

export interface AppActivation {
  enabled?: boolean;
  title?: string;
  message?: string;
                                                                     
  provider?: "tb" | string;
  items: AppActivationItem[];
}

                                     
export interface TbActivationResponse {
  ok: boolean;
  id?: string;
  name?: string;
  scheme?: string;
  qr_payload?: string;
  mode?: ActivationMode;
  app_store_id?: string;
  bundle_id?: string;
  hint?: string;
                                                          
  expires_in?: number;
  ttl_seconds?: number;
  from_cache?: boolean;
  error?: string;
}

export interface CatalogGroup {
  id: string;
  title: string;
  priority?: number;
}

export interface CatalogBanner {
  id: string;
  title: string;
  text?: string;
  style?: string;
  app_id?: string | null;
  action_label?: string | null;
  priority?: number;
  active?: boolean;
}

export interface CatalogFeed {
  apps: CatalogApp[];
  groups: CatalogGroup[];
  banners: CatalogBanner[];
  from_cache?: boolean;
  cached_at?: number | null;
}

export interface CatalogApp {
  id: string;
  title: string;
  icon_url?: string;
  badge?: Badge;
  version?: string;
  min_ios?: string;
  active?: boolean;
  in_store?: boolean;
  color?: string;
  aliases?: string[];
                                        
  category?: string;
                                                  
  activation?: AppActivation | null;
  /** Delivered through TestFlight instead of a signed IPA. */
  testflight?: TestFlightOffer | null;
}

export interface TestFlightOffer {
  /** Public invite link, e.g. https://testflight.apple.com/join/XXXXXXX */
  url: string;
}

                                                                            
/** Разработчик из App Store — для поиска по студии, а не по названию. */
export interface StoreDeveloper {
  artistId: string;
  name: string;
  /** Сколько приложений нашлось: видно масштаб студии прямо в списке. */
  appCount: number;
  /** До трёх значков для предпросмотра в строке результата. */
  icons: string[];
}

export interface StoreSearchApp {
  name: string;
  publisher: string;
  iconUrl: string;
  appStoreId: string;
                                                                   
  removedFromStore?: boolean;
  foundOnAccount?: boolean;
}

export interface AppStoreVersion {
  version: string;
  externalVersionId: string;
  isLatest: boolean;
  alreadyOwned: boolean;

  minIos?: string;
}

export interface AppStoreVersionsPage {
  versions: AppStoreVersion[];
  hasMore: boolean;
  total: number;
  offset: number;
}

export interface AccountRestoreVersion {
  app_id: string;
  app_title: string;
  title: string;
  description?: string;
  version: string;
  bundle_id: string;
  app_store_id?: string;
  min_ios?: string;
  icon_url?: string;
  in_store: boolean;
  device_compatible?: boolean | null;
                                                        
  installed_on_device?: boolean;
  installed_version?: string;
}

export interface AccountRestoreScan {
  device_ios?: string;
  items: AccountRestoreVersion[];
                                                                    
  message?: string;
}

                                                                        
export interface PurchaseHistoryApp {
  adamId: string;
  name: string;
  artist?: string;
  artwork?: string;
  price?: string;
  date?: string;
  bundleId?: string;
}

export interface PurchaseHistoryResult {
  email: string;
  count: number;
  apps: PurchaseHistoryApp[];
  message?: string | null;
                                                              
  needs2fa?: boolean;
                                                             
  hasMore?: boolean;
                                                       
  nextBatchId?: string | null;
                                                             
  sessionSaved?: boolean;
                                      
  fromCache?: boolean;
                                       
  debugLogPath?: string | null;
}

export interface AppVersion {
  version: string;
  label?: string;
  title?: string;
  description?: string;
  icon_url?: string;
  badge?: Badge;
  bundle_id?: string;
  app_store_id?: string;
  min_ios?: string;
  is_latest?: boolean;
  in_store?: boolean;
}

export interface AppInfo extends CatalogApp {
  versions?: AppVersion[];
}

export interface CatalogInstallOffer {
  app_id: string;
  app_title: string;
  title: string;
  description?: string;
  version: string;
  bundle_id: string;
  app_store_id?: string;
  min_ios?: string;
  icon_url?: string;
  in_store: boolean;
  source: "app_store" | "restore";
  message: string;
  show_account_versions: boolean;
                                                        
  installed_on_device?: boolean;
  installed_version?: string;
                                                                      
  device_compatible?: boolean | null;
  device_ios?: string;
}

export interface DeviceInfo {
  udid: string;
  product_type?: string;
  product_version?: string;
  device_name?: string;
  battery_current_capacity?: number;
                                                       
  connection?: "usb" | "wifi" | "both" | string;
}

export interface InstalledApp {
  bundle_id: string;
  name?: string;
  version?: string;
  app_store_id?: string | null;
  is_placeholder?: boolean;
}

export interface TransferAppItem {
  name: string;
  catalog_id?: string | null;
  bundle_id?: string | null;
  app_store_id?: string | null;
  version?: string | null;
}

export interface TransferPreview {
  target_device: string;
  target_udid: string;
  items: { item: TransferAppItem; status: "ready" | "installed" | "unavailable" | "incompatible"; message: string; min_ios?: string | null }[];
}

export interface BulkInstallResult {
  jobIds: string[];
  skipped: { appId: string; reason: string }[];
}

export interface CachedIpa {
  filename: string;
  path: string;
  bundle_id?: string;
  name?: string;
  version?: string;
  apple_id?: string | null;
  size: number;
  modified_at: number;
  device_compatible?: boolean | null;
}

export type JobStatus = "queued" | "running" | "done" | "failed" | "cancelled";
export type InstallJobKind = "install" | "export" | "open_exports";
export type SigningCertMode = "auto" | "free" | "paid" | "custom";

export interface SavedCertInfo {
  name: string;
  p12_path: string;
  prov_path: string;
  has_password: boolean;

  expires_at?: string | null;
}

                                                                     
export type BankKey = "sovcombank" | "rocketbank";

                                                            
export interface SignOptions {
  lower_min_ios?: boolean;
  min_ios_version?: string | null;
  file_sharing?: boolean;
  remove_device_limits?: boolean;
  change_app_name?: boolean;
  app_name?: string | null;
  change_version?: boolean;
  app_version?: string | null;
                                                     
  auto_bundle_id?: boolean;
  custom_bundle_id?: string | null;
                                                   
  force_unique_bundle?: boolean;
  strip_plugins?: boolean;
  strip_watch?: boolean;
  clear_url_schemes?: boolean;
  documents_in_place?: boolean;
  supports_opening_docs?: boolean;
  allow_http?: boolean;
  remove_orientation_lock?: boolean;
  strip_frameworks?: boolean;
}

export interface IpaInspect {
  kind: "encrypted" | "signed" | "decrypted" | "invalid" | string;
  title: string;
  explanation: string;
  needs_sign: boolean;
  app_name?: string | null;
  version?: string | null;
  bundle_id?: string | null;
  min_ios?: string | null;
  architectures?: string[];
  extensions?: string[];
  profile_expires_at?: string | null;
  warnings?: string[];
}

export interface SignAndInstallInput {
  ipa_path: string;
  udid?: string | null;
  install?: boolean;
  save_signed?: boolean;
  output_dir?: string | null;
  cert_mode?: SigningCertMode;
  cert_name?: string | null;
  custom_p12_path?: string | null;
  custom_prov_path?: string | null;
  custom_cert_password?: string | null;
  options?: SignOptions;
}

export interface SigningStatus {
  authenticated: boolean;
  email?: string | null;
  team_id?: string | null;
  team_name?: string | null;
  team_kind?: string | null;
  cert_ready: boolean;
  message?: string | null;
}

export interface InstallJob {
  id: string;
  title: string;
  subtitle: string;
  phase: string;
  percent: number;
  message: string;
  status: JobStatus;
  error?: string;
  created_at: number;
  kind?: InstallJobKind;
  retryable?: boolean;
}

export interface ProgressEvent {
  job_id: string;
  phase: string;
  percent: number;
  message: string;
  status: JobStatus;
  error?: string;
}

export type SessionState = "active" | "saved" | "expired" | "needs_2fa" | "damaged" | "not_saved";

export interface AppleAccountInfo {
  email: string;
  active: boolean;
  session_saved: boolean;
  session_state: SessionState;
  added_at: number;
  session_error?: string | null;
}

export interface ActionHistoryEntry {
  id: string;
  account_email?: string;
  device_udid?: string;
  title: string;
  subtitle: string;
  status: string;
  kind: string;
  finished_at: number;
}

export interface StoreRegionOption {
  code: string;
  name: string;
}

export interface GeneratedAddress {
  countryCode: string;
  countryName: string;
  street: string;
  city: string;
  postalCode: string;
  state?: string | null;
  county?: string | null;
  phoneArea: string;
  phoneNumber: string;
}

export interface StoreRegionStatus {
    countryCode: string;
    countryName: string;
    needs2fa: boolean;
    message?: string | null;
}

                                                                                  
export interface StoreRegionCacheEntry {
    email: string;
    countryCode: string;
    countryName: string;
                                     
    timestamp: number;
}

export interface ChangeRegionResult {
  ok: boolean;
  needs2fa: boolean;
  countryCode: string;
  countryName: string;
  message: string;
}

export interface AuthStatus {
  authenticated: boolean;
  email?: string;
  accounts: AppleAccountInfo[];
                                                                         
  historyPasswordSaved?: boolean;
                                                             
  historySessionSaved?: boolean;
  catalogError?: string | null;
}

                                                                 
export interface LoginAppleResult {
  email: string;
                              
  needs2fa?: boolean;
                                                                   
  historyNeeds2fa?: boolean;
                          
  historyReady?: boolean;
                                               
  historyError?: string | null;
  message?: string | null;
  catalogError?: string | null;
}

export type IpatoolKeychainMode = "random" | "custom";
/**
 * Режимы загрузки. v2 и v3 убраны 15.09.2026: ни один не знает ступени
 * updateProduct, без которой Apple не отдаёт сборки, выпущенные с начала
 * сентября. Сохранённый у пользователя выбор "v2"/"v3" приводится к "auto".
 */
export type IpatoolEngine = "auto" | "v4" | "v5";

export interface IpatoolEngineStatus {
  selected: IpatoolEngine;
  active?: IpatoolEngine | null;
}

export interface StoreRegionPlan {

  action: "owned" | "purchase" | "switch" | "unavailable";
  accountRegion?: string | null;
  accountRegionName?: string | null;
  targetRegion?: string | null;
  targetRegionName?: string | null;
  message: string;
}

export interface SignedAppRecord {
  bundle_id: string;
  title: string;
  ipa_path: string;
  udid?: string | null;
  cert_name?: string | null;
  team_kind: string;
  expires_at?: string | null;
  expires_unix?: number | null;
  signed_at: number;
}

export interface AppSettings {
  /** Автозапуск в трей и поддержание кеша FairPlay горячим. */
  background_enabled: boolean;
  /** Прокси для обращений к Apple: и наших, и движка загрузки. */
  proxy_enabled: boolean;
  /** socks5 | socks4 | http | https */
  proxy_kind: string;
  proxy_host: string;
  proxy_port: number;
  proxy_user: string;
  proxy_password: string;
  favorite_app_ids?: string[];
  auto_resign_before_expiry?: boolean;
  downloads_dir: string;
  exports_dir: string;
  theme: "light" | "dark" | "system";
  auto_update: boolean;
                                                                                 
  auto_purchase_new_apps: boolean;
  lite_design: boolean;
  detailed_errors: boolean;
  auto_diag: boolean;
  lite_static_loaders: boolean;
  lite_reduce_motion: boolean;
  lite_simple_scrolling: boolean;
  lite_disable_blur: boolean;
  lite_disable_shadows: boolean;
  lite_flat_backgrounds: boolean;
  ipatool_keychain_mode: IpatoolKeychainMode;
  ipatool_keychain_passphrase?: string;
                                                                               
  ipatool_engine: IpatoolEngine;
  selected_device_udid?: string;
                                             
  selected_device_connection?: string;
                                                                                  
  keep_history_session?: boolean;
}

export interface AppVersionInfo {
  version: string;
  build: string;
  beta: boolean;
}

export interface OtaUpdateInfo {
  has_update: boolean;
  version?: string;
  build?: string;
  download_url?: string;
    notes?: string;
  mandatory: boolean;
                                                       
  platform?: "windows" | "linux" | "macos" | "android" | "unknown";
                                                                        
  release_available?: boolean;
                                                                               
  sha256?: string;
                                                    

  min_client_version?: string;
                                                                   
  client_supported?: boolean;
}

export interface ClientOutdatedInfo {
  message: string;
  min_client_version?: string;
    latest_version?: string;
  download_url?: string;
  sha256?: string;

}

export interface AppUpdateFoundEvent {
  app_id: string;
  title: string;
  bundle_id: string;
  installed_version: string;
  remote_version: string;
  in_store: boolean;
  app_store_id?: string;
  min_ios?: string;
}
