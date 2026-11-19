import { useEffect, useMemo, useState } from "react";
import { Icon, RailButton, Viewer } from "@wa/viewer";
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

  const other = stores.find((s) => s.sourceApp !== store);
  const actions = other && (
    <button
      className="icon-btn"
      title={`Trocar para ${SOURCE_LABEL[other.sourceApp]}`}
      onClick={() => setStore(other.sourceApp)}
    >
      <Icon name="swap" size={22} />
    </button>
  );
  const railBottom = (
    <>
      <RailButton icon="import" title="Importar ou exportar" onClick={onOpenImport} />
      <RailButton icon="lock" title="Travar o cofre" onClick={onLock} />
    </>
  );

  return (
    <Viewer
      key={`${store}:${stamp}`}
      source={source}
      title={SOURCE_LABEL[store]}
      actions={actions}
      railBottom={railBottom}
      emptyTitle={`Histórico do ${SOURCE_LABEL[store]}`}
      emptyNote="Os dados ficam cifrados neste computador e não saem dele."
    />
  );
}
