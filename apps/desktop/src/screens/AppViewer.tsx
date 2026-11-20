import { useEffect, useMemo, useState } from "react";
import { Icon, RailButton, Viewer } from "@wk/viewer";
import { SOURCE_LABEL, SourceApp, StoreInfo, tauriSource } from "../api";
import { useAppT } from "../i18n/app";
import { JobBanner } from "../jobs";

const SHORT_LABEL: Record<SourceApp, string> = { whatsapp: "WhatsApp", business: "Business" };

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

  // Both histories imported: a switcher with both names, always in sight.
  const switcher = stores.length > 1 && (
    <div className="app-switch" role="radiogroup" aria-label={t.switchLabel}>
      {stores.map((s) => (
        <button
          key={s.sourceApp}
          role="radio"
          aria-checked={s.sourceApp === store}
          className={s.sourceApp === store ? "active" : ""}
          onClick={() => setStore(s.sourceApp)}
        >
          <Icon name={s.sourceApp === "business" ? "business" : "whatsapp"} size={16} />
          {SHORT_LABEL[s.sourceApp]}
        </button>
      ))}
    </div>
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
      subheader={switcher || undefined}
      railBottom={railBottom}
      banner={<JobBanner onDetails={onOpenImport} />}
      emptyTitle={t.emptyTitle(SOURCE_LABEL[store])}
      emptyNote={t.emptyNote}
    />
  );
}
