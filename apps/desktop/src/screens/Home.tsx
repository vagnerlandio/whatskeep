import { useState } from "react";
import { formatDateTime, Icon, IconName, useLang } from "@wk/viewer";
import { api, formatSize, SOURCE_LABEL, SourceApp, StoreInfo } from "../api";
import { useAppT } from "../i18n/app";
import { useImportT } from "../i18n/import";
import { JobCard, useJob } from "../jobs";
import { useVaultGate } from "../vault";
import ConfirmDelete from "./ConfirmDelete";
import DriveImport from "./DriveImport";
import ExportPanel from "./ExportPanel";
import MediaFolderImport from "./MediaFolderImport";
import ImportPanel from "./ImportPanel";
import { Panel, Section } from "./ui";

type Props = {
  stores: StoreInfo[];
  onClose?: () => void;
  onStoresChanged: () => void;
  onVaultDeleted: () => void;
};

type Deleting = { kind: "store"; store: StoreInfo } | { kind: "vault"; size: number | null };
type View = "choose" | "drive" | "file" | "export";

/**
 * Where a history comes from: Google Drive or a backup on the computer. Also
 * lists the histories in the vault and the export. Start screen when the vault
 * is empty (or does not exist yet).
 */
export default function Home({ stores, onClose, onStoresChanged, onVaultDeleted }: Props) {
  const t = useAppT();
  const ti = useImportT();
  const { lang } = useLang();
  const { job } = useJob();
  const { vault } = useVaultGate();
  const [view, setView] = useState<View>("choose");
  const [deleting, setDeleting] = useState<Deleting | null>(null);
  const busy = job?.status === "running";

  async function askDeleteVault() {
    setDeleting({ kind: "vault", size: null });
    try {
      const size = await api.usage();
      setDeleting((d) => (d?.kind === "vault" ? { kind: "vault", size } : d));
    } catch {
      // The size is only informative.
    }
  }

  async function deleteStore(store: SourceApp) {
    await api.deleteStore(store);
    setDeleting(null);
    onStoresChanged();
  }

  return (
    <div className="home">
      {onClose && view === "choose" && (
        <button className="back" onClick={onClose}>
          <Icon name="back" size={16} /> {t.backToHistory}
        </button>
      )}
      <JobCard onViewChats={stores.length > 0 ? onClose : undefined} />
      {/* Hidden, not unmounted, during a job: the Google sign-in survives a retry. */}
      <div className="home-panels" hidden={job !== null}>
        {view === "choose" ? (
          <>
            <section className="card">
              <h2>{ti.hubTitle}</h2>
              {vault === "none" && <p className="muted small">{ti.hubNoVault}</p>}
              <div className="choices">
                <Choice icon="cloud" title={ti.hubDriveTitle} text={ti.hubDriveText} onClick={() => setView("drive")} />
                <Choice icon="folder" title={ti.hubFileTitle} text={ti.hubFileText} onClick={() => setView("file")} />
              </div>
            </section>
            {stores.length > 0 && (
              <Panel>
                {stores.map((s, i) => (
                  <Section key={s.sourceApp} label={i === 0 ? t.storesTitle : undefined}>
                    <div className="store-row">
                      <span className="store-avatar">
                        <Icon name="chats" size={22} />
                      </span>
                      <span className="store-info">
                        <strong>{SOURCE_LABEL[s.sourceApp]}</strong>
                        <span className="small">
                          {t.chats(s.chats)} · {t.messages(s.messages)}
                        </span>
                        <span className="muted small">{t.importedAt(formatDateTime(s.importedAtMs, lang))}</span>
                      </span>
                      <span className="store-actions">
                        <MediaFolderImport source={s.sourceApp} />
                        <button
                          className="small-btn ghost-danger"
                          disabled={busy}
                          title={t.deleteStore}
                          onClick={() => setDeleting({ kind: "store", store: s })}
                        >
                          <Icon name="trash" size={14} /> {t.remove}
                        </button>
                      </span>
                    </div>
                  </Section>
                ))}
              </Panel>
            )}
            {stores.length > 0 && (
              <Panel>
                <Section label={t.exportTitle}>
                  <div className="store-row">
                    <span className="store-avatar">
                      <Icon name="document" size={22} />
                    </span>
                    <span className="store-info muted small">{t.exportIntro}</span>
                    <button className="small-btn" disabled={busy} onClick={() => setView("export")}>
                      {t.configureExport}
                    </button>
                  </div>
                </Section>
              </Panel>
            )}
            {vault === "unlocked" && (
              <Panel className="danger-zone">
                <Section label={t.deleteVaultHeading}>
                  <div className="store-row">
                    <span className="store-info muted small">{t.deleteVaultCard}</span>
                    <button className="small-btn ghost-danger" disabled={busy} onClick={askDeleteVault}>
                      <Icon name="trash" size={14} /> {t.deleteVault}
                    </button>
                  </div>
                </Section>
              </Panel>
            )}
          </>
        ) : view === "export" ? (
          <ExportPanel stores={stores} onBack={() => setView("choose")} />
        ) : view === "drive" ? (
          <DriveImport stores={stores} onBack={() => setView("choose")} />
        ) : (
          <ImportPanel stores={stores} onBack={() => setView("choose")} />
        )}
      </div>
      {deleting?.kind === "store" && (
        <ConfirmDelete
          title={t.deleteStoreTitle(SOURCE_LABEL[deleting.store.sourceApp])}
          onClose={() => setDeleting(null)}
          onConfirm={() => deleteStore(deleting.store.sourceApp)}
        >
          <p>{t.deleteStoreWhat(t.chats(deleting.store.chats), t.messages(deleting.store.messages))}</p>
          <p className="muted small">{t.deleteVaultKept}</p>
        </ConfirmDelete>
      )}
      {deleting?.kind === "vault" && (
        <ConfirmDelete
          title={t.deleteVaultTitle}
          onClose={() => setDeleting(null)}
          onConfirm={async () => {
            await api.deleteVault();
            setDeleting(null);
            onVaultDeleted();
          }}
        >
          <p>{t.deleteVaultWhat(deleting.size === null ? "…" : formatSize(deleting.size, lang))}</p>
          <p className="muted small">{t.deleteVaultKept}</p>
        </ConfirmDelete>
      )}
    </div>
  );
}

function Choice({ icon, title, text, onClick }: { icon: IconName; title: string; text: string; onClick: () => void }) {
  return (
    <button className="choice" onClick={onClick}>
      <Icon name={icon} size={28} />
      <strong>{title}</strong>
      <span className="muted small">{text}</span>
    </button>
  );
}

/** Locked vault: saves a Google Drive backup copy to a folder without opening it. */
export function DownloadOnly({ onBack }: { onBack: () => void }) {
  const t = useAppT();
  const { job } = useJob();
  return (
    <div className="home">
      {job && (
        <button className="back" onClick={onBack}>
          <Icon name="back" size={16} /> {t.backToUnlock}
        </button>
      )}
      <JobCard />
      <div className="home-panels" hidden={job !== null}>
        <DriveImport stores={[]} folderOnly onBack={onBack} backLabel={t.backToUnlock} />
      </div>
    </div>
  );
}
