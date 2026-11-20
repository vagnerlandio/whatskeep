// Command errors in the UI language.
//
// The backend returns `{ code, message }`: `code` is stable and `message` is a
// Portuguese text, sometimes with details (name of the network error etc.).
// In Portuguese, shows the backend's message (more specific). In English,
// one text per code; the technical detail goes to the diagnostics log.
import { errorOf, makeT, useLang } from "@wk/viewer";

const en: Record<string, string> = {
  locked: "The vault is locked.",
  weak_password: "The password is too short (at least 8 characters).",
  wrong_password: "Wrong password.",
  already_exists: "A vault already exists on this computer.",
  not_found: "Not found.",
  vault_outdated: "This vault was created by an earlier development version of WhatsKeep and cannot be opened. Delete the vault folder and create a new one.",
  vault_corrupt: "The vault could not be read (damaged or unsupported files).",
  not_msgstore: "This file is not a WhatsApp message database (msgstore).",
  invalid_key: "Invalid key. Check the 64 digits or the key file.",
  key_mismatch: "This key does not match the backup type (crypt14 needs the key file, crypt15 the 64-digit key).",
  wrong_key: "The key does not open this backup. Check that it belongs to this account and app.",
  invalid_backup: "The backup file is damaged or in an unknown format.",
  io: "Could not read or write the file.",
  google_auth: "Google rejected the sign-in. Try signing in again (the sign-in code expires in a few minutes).",
  google_expired: "The Google session expired. Sign in again.",
  download_corrupt: "The download arrived incomplete or corrupted. Try again.",
  network: "Network error. Check your connection and try again.",
  google: "Google returned an unexpected error.",
  busy: "Another operation is already in progress.",
  no_store: "No history has been imported for this app yet.",
  invalid_argument: "Invalid input.",
  cancelled: "Cancelled.",
  internal: "Something went wrong. Details are in the diagnostics log.",
};

/** Error text in the current language. */
export function useErrorText(): (e: unknown) => string {
  const { lang } = useLang();
  return (e: unknown) => {
    const { code, message } = errorOf(e);
    if (lang === "pt-BR") return message;
    return en[code] ?? `Something went wrong (${code}).`;
  };
}

// Re-exports the dictionary helper for the app's screens.
export { makeT };
