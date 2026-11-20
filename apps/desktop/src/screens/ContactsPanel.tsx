import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { formatCount, formatDateTime, Icon, useLang } from "@wk/viewer";
import { contactsApi, ContactsInfo } from "../api";
import { useAppT } from "../i18n/app";
import { useErrorText } from "../i18n/errors";
import { More, Note, Panel, Section } from "./ui";

/** Contact names from a Google Contacts export, kept encrypted in the vault. */
export default function ContactsPanel({ busy, onChanged }: { busy: boolean; onChanged: () => void }) {
  const t = useAppT();
  const { lang } = useLang();
  const errorText = useErrorText();
  const [info, setInfo] = useState<ContactsInfo | null | undefined>(undefined);
  const [working, setWorking] = useState(false);
  // Raw error: the text follows language changes.
  const [error, setError] = useState<{ e: unknown } | null>(null);

  useEffect(() => {
    contactsApi.info().then(setInfo, (e) => setError({ e }));
  }, []);

  async function run(action: () => Promise<ContactsInfo | null>) {
    setWorking(true);
    setError(null);
    try {
      setInfo(await action());
      onChanged();
    } catch (e) {
      setError({ e });
    } finally {
      setWorking(false);
    }
  }

  async function importFile() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t.contactsFilter, extensions: ["csv", "vcf"] }],
    });
    if (path) await run(() => contactsApi.import(path));
  }

  if (info === undefined && !error) return null;
  const disabled = busy || working;
  const n = (v: number) => formatCount(v, lang);

  return (
    <Panel>
      <Section label={t.contactsTitle}>
        <div className="store-row">
          <span className="store-avatar">
            <Icon name="contact" size={22} />
          </span>
          {info ? (
            <span className="store-info">
              <span className="small">{t.contactsSummary(n(info.people), n(info.namedChats))}</span>
              <span className="muted small">{t.contactsImportedAt(formatDateTime(info.importedAtMs, lang))}</span>
            </span>
          ) : (
            <span className="store-info muted small">{working ? t.contactsWorking : t.contactsNone}</span>
          )}
          <span className="store-actions">
            {info && (
              <button className="small-btn ghost-danger" disabled={disabled} onClick={() => run(async () => (await contactsApi.remove(), null))}>
                <Icon name="trash" size={14} /> {t.contactsRemove}
              </button>
            )}
            <button className="small-btn" disabled={disabled} onClick={importFile}>
              {info ? t.contactsUpdate : t.contactsImport}
            </button>
          </span>
        </div>
        {error && <Note tone="danger">{errorText(error.e)}</Note>}
        <More summary={t.contactsHowTo}>
          <ol className="steps">
            <li>{t.contactsStep1}</li>
            <li>{t.contactsStep2}</li>
            <li>{t.contactsStep3}</li>
          </ol>
        </More>
      </Section>
    </Panel>
  );
}
