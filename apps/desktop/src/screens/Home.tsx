import { formatDateTime, Icon, useLang } from "@wa/viewer";
import { SOURCE_LABEL, StoreInfo } from "../api";
import { useAppT } from "../i18n/app";
import DriveImport from "./DriveImport";
import ExportPanel from "./ExportPanel";
import MediaFolderImport from "./MediaFolderImport";
import ImportPanel from "./ImportPanel";

type Props = { stores: StoreInfo[]; onImported: () => void; onClose?: () => void };

/** Importação e lista dos históricos no cofre. Tela inicial quando o cofre está vazio. */
export default function Home({ stores, onImported, onClose }: Props) {
  const t = useAppT();
  const { lang } = useLang();
  return (
    <div className="home">
      {onClose && (
        <button className="back" onClick={onClose}>
          <Icon name="back" size={16} /> {t.backToHistory}
        </button>
      )}
      <section className="card">
        <h2>{t.storesTitle}</h2>
        {stores.length === 0 && <p className="muted">{t.noStores}</p>}
        {stores.map((s) => (
          <div className="store" key={s.sourceApp}>
            <strong>{SOURCE_LABEL[s.sourceApp]}</strong>
            <span>
              {t.chats(s.chats)} · {t.messages(s.messages)}
            </span>
            <span className="muted small">{t.importedAt(formatDateTime(s.importedAtMs, lang))}</span>
            <MediaFolderImport source={s.sourceApp} />
          </div>
        ))}
      </section>
      <DriveImport stores={stores} onImported={onImported} />
      <ImportPanel stores={stores} onImported={onImported} />
      {stores.length > 0 && <ExportPanel stores={stores} />}
    </div>
  );
}
