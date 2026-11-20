import { useEffect, useMemo, useState } from "react";
import { Icon, RailButton, Viewer } from "@wk/viewer";
import { SOURCE_LABEL, SourceApp, StoreInfo, tauriSource } from "../api";
import { useAppT } from "../i18n/app";

type Props = {
  stores: StoreInfo[];
  onLock: () => void;
  onOpenImport: () => void;
};

/** The shared viewer on top of the vault, with the app's controls. */
export default function AppViewer({ stores, onLock, onOpenImport }: Props) {
  const t = useAppT();
  const [store, setStore] = useState<SourceApp>(stores[0].sourceApp);

  // If the selected database disappears (it shouldn't), falls back to the first one.
  useEffect(() => {
    if (!stores.some((s) => s.sourceApp === store)) setStore(stores[0].sourceApp);
  }, [stores, store]);

  const stamp = stores.find((s) => s.sourceApp === store)?.importedAtMs ?? 0;
  // New source when switching app or reimporting: the viewer starts over.
  const source = useMemo(() => tauriSource(store), [store, stamp]);

  const other = stores.find((s) => s.sourceApp !== store);
  const actions = other && (
    <button
      className="icon-btn"
      title={t.switchTo(SOURCE_LABEL[other.sourceApp])}
      onClick={() => setStore(other.sourceApp)}
    >
      <Icon name="swap" size={22} />
    </button>
  );
  const railBottom = (
    <>
      <RailButton icon="import" title={t.importOrExport} onClick={onOpenImport} />
      <RailButton icon="lock" title={t.lockTitle} onClick={onLock} />
    </>
  );

  return (
    <Viewer
      key={`${store}:${stamp}`}
      source={source}
      title={SOURCE_LABEL[store]}
      actions={actions}
      railBottom={railBottom}
      emptyTitle={t.emptyTitle(SOURCE_LABEL[store])}
      emptyNote={t.emptyNote}
    />
  );
}
