   
                                                                 
                                                                              
                                                                 
   
const TWO_FACTOR_REQUIRED_MARKERS = [
  "signum_2fa_required",
  "2fa code is required",
  "auth code is required",
  "two-factor authentication is required",
  "two factor authentication is required",
  "secondary authentication required",
  "enter the verification code",
  "enter verification code",
  "enter the code to continue",
  "enter 2fa code",
  "verification code has been sent",
  "[2fa]",
                      
  "two-factor auth code required",
  "two factor auth code required",
  "retry with --auth-code",
  "auth-code required",
  "auth code required",
  "запросил код подтверждения",
  "требуется код подтверждения",
  "нужен код подтверждения",
];

export function isTwoFactorError(message: string): boolean {
  const lowered = message.toLowerCase();
  if (lowered.startsWith("signum_2fa_required")) return true;
  return TWO_FACTOR_REQUIRED_MARKERS.some(marker => lowered.includes(marker));
}

export function formatAuthError(message: string): string {
  const text = (message || "").trim();
  const lowered = text.toLowerCase();
  if (text.startsWith("SIGNUM_2FA_REQUIRED:") || isTwoFactorError(text)) {
    const technical = text
      .replace(/^SIGNUM_2FA_REQUIRED:\s*/i, "")
      .trim();
    return technical && technical !== "Apple запросил код подтверждения. Введите 6 цифр с доверенного устройства."
      ? `Введите 6-значный код с iPhone.\n\n${technical}`
      : "Введите 6-значный код с iPhone.";
  }
  if (!text) {
    return "Сейчас не получается подтвердить вход. Попробуйте позже или войдите снова.";
  }
  if (
    lowered.includes("incorrect password")
    || lowered.includes("wrong password")
    || lowered.includes("invalid password")
    || lowered.includes("неверный пароль")
    || lowered.includes("неверный apple id")
  ) {
    return text.includes("Технически:")
      ? text
      : `Не удалось войти. Проверьте Apple ID и пароль.\n\n${text}`;
  }
  return text;
}
