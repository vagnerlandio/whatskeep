// The vault is created only when something is about to go into it: saving a
// backup copy to a folder needs no password.
import { createContext, ReactNode, useCallback, useContext, useState } from "react";
import { useAppT } from "./i18n/app";
import CreateVault from "./screens/CreateVault";

export type VaultState = "none" | "unlocked";

type Gate = {
  vault: VaultState;
  /** Runs `action` once the vault exists, asking for its password first if it doesn't. */
  requireVault: (action: () => void) => void;
};

const GateContext = createContext<Gate | null>(null);

export function useVaultGate(): Gate {
  const ctx = useContext(GateContext);
  if (!ctx) throw new Error("useVaultGate outside VaultGateProvider");
  return ctx;
}

/** `null` outside the provider (locked vault: only folder downloads are possible there). */
export function useOptionalVaultGate(): Gate | null {
  return useContext(GateContext);
}

export function VaultGateProvider({
  vault,
  autolockMinutes,
  onCreated,
  children,
}: {
  vault: VaultState;
  autolockMinutes: number;
  onCreated: () => void;
  children: ReactNode;
}) {
  const t = useAppT();
  // The action waiting for the vault (kept in a box: React would call a bare function).
  const [pending, setPending] = useState<{ run: () => void } | null>(null);

  const requireVault = useCallback(
    (action: () => void) => (vault === "unlocked" ? action() : setPending({ run: action })),
    [vault],
  );

  return (
    <GateContext.Provider value={{ vault, requireVault }}>
      {children}
      {pending && (
        <div className="modal-backdrop" role="dialog" aria-modal>
          <CreateVault
            autolockMinutes={autolockMinutes}
            reason={t.createBeforeImport}
            onCancel={() => setPending(null)}
            onCreated={() => {
              setPending(null);
              // The backend is unlocked as soon as the vault is created.
              pending.run();
              onCreated();
            }}
          />
        </div>
      )}
    </GateContext.Provider>
  );
}
