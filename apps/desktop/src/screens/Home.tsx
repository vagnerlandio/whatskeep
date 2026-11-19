import { formatCount, SOURCE_LABEL, StoreInfo } from "../api";
import DriveImport from "./DriveImport";
import ExportPanel from "./ExportPanel";
import MediaFolderImport from "./MediaFolderImport";
import ImportPanel from "./ImportPanel";

type Props = { stores: StoreInfo[]; onImported: () => void; onClose?: () => void };

/** Importação e lista dos históricos no cofre. Tela inicial quando o cofre está vazio. */
export default function Home({ stores, onImported, onClose }: Props) {
  return (
    <div className="home">
      {onClose && (
        <button className="back" onClick={onClose}>
          ← Voltar ao histórico
        </button>
      )}
      <section className="card">
        <h2>Históricos no cofre</h2>
        {stores.length === 0 && <p className="muted">Nenhum backup importado ainda.</p>}
        {stores.map((s) => (
          <div className="store" key={s.sourceApp}>
            <strong>{SOURCE_LABEL[s.sourceApp]}</strong>
            <span>
              {formatCount(s.chats)} conversas · {formatCount(s.messages)} mensagens
            </span>
            <span className="muted small">Importado em {new Date(s.importedAtMs).toLocaleString("pt-BR")}</span>
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
