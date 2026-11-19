import { useEffect, useMemo, useState } from "react";
import { Viewer } from "@wa/viewer";
import { SOURCE_LABEL, SourceApp, StoreInfo, tauriSource } from "../api";

type Props = {
  stores: StoreInfo[];
  onLock: () => void;
  onOpenImport: () => void;
};

/** O visualizador compartilhado sobre o cofre, com os controles do app. */
export default function AppViewer({ stores, onLock, onOpenImport }: Props) {
  const [store, setStore] = useState<SourceApp>(stores[0].sourceApp);

  // Se o banco selecionado sumir (não deveria), volta para o primeiro.
  useEffect(() => {
    if (!stores.some((s) => s.sourceApp === store)) setStore(stores[0].sourceApp);
  }, [stores, store]);

  const stamp = stores.find((s) => s.sourceApp === store)?.importedAtMs ?? 0;
  // Fonte nova quando troca o app ou reimporta: o visualizador recomeça.
  const source = useMemo(() => tauriSource(store), [store, stamp]);

  const header = (
    <>
      {stores.length > 1 ? (
        <div className="store-switch">
          {stores.map((s) => (
            <button
              key={s.sourceApp}
              className={s.sourceApp === store ? "active" : ""}
              onClick={() => setStore(s.sourceApp)}
            >
              {s.sourceApp === "business" ? "Business" : "WhatsApp"}
            </button>
          ))}
        </div>
      ) : (
        <strong className="sidebar-title">{SOURCE_LABEL[store]}</strong>
      )}
      <div className="sidebar-actions">
        <button className="icon-btn" title="Importar ou exportar" onClick={onOpenImport}>
          ＋
        </button>
        <button className="icon-btn" title="Travar o cofre" onClick={onLock}>
          🔒
        </button>
      </div>
    </>
  );

  return (
    <Viewer
      key={`${store}:${stamp}`}
      source={source}
      header={header}
      emptyTitle={`Histórico do ${SOURCE_LABEL[store]}`}
      emptyNote="🔒 Os dados ficam cifrados neste computador e não saem dele."
    />
  );
}
