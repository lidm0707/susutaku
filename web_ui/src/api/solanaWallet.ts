// Thin React wrapper over the solana_wallet wasm pkg.
import init, { SusuWallet } from "../wallet-pkg";
import { useCallback, useEffect, useState } from "react";

let initPromise: Promise<unknown> | null = null;

async function loadWallet(): Promise<SusuWallet> {
  if (!initPromise) initPromise = init();
  await initPromise;
  return new SusuWallet();
}

export interface WalletState {
  address: string;
  balanceSol: number | null;
  refresh: () => Promise<void>;
  airdrop: (sol: number) => Promise<string>;
}

export function useSolanaWallet(): WalletState | null {
  const [wallet, setWallet] = useState<SusuWallet | null>(null);
  const [balanceSol, setBalanceSol] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadWallet().then((w) => {
      if (!cancelled) setWallet(w);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const refresh = useCallback(async () => {
    if (!wallet) return;
    setBalanceSol(await wallet.balance_sol());
  }, [wallet]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (!wallet) return null;

  return {
    address: wallet.address,
    balanceSol,
    refresh,
    airdrop: (sol: number) => wallet.airdrop(BigInt(Math.round(sol * 1_000_000_000))),
  };
}
