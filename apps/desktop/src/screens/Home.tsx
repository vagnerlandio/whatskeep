import { useCallback, useEffect, useState } from "react";
import { api, errorOf, formatCount, SOURCE_LABEL, StoreInfo } from "../api";
import ImportPanel from "./ImportPanel";

export default function Home() {
  const [stores, setStores] = useState<StoreInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStores(await api.listStores());
      setError(null);
    } catch (e) {
      setError(errorOf(e).message);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="home">
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
          </div>
        ))}
        {error && <div className="result error">{error}</div>}
      </section>
      <ImportPanel stores={stores} onImported={refresh} />
    </div>
  );
}
