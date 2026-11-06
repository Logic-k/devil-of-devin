// 자동 업데이트: GitHub Release 의 latest.json 을 확인하고, 사용자가 수락하면 내려받아 설치 후 재시작한다.
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { useStore } from "../store";

let pending: Update | null = null;

export async function currentVersion(): Promise<string> {
  try { return await getVersion(); } catch { return "dev"; }
}

/** 업데이트 확인. manual=true 면 결과를 토스트로 알린다. */
export async function checkForUpdate(manual = false): Promise<void> {
  const st = useStore.getState();
  if (import.meta.env.DEV && !manual) return; // 개발 모드 자동 확인 생략
  st.set({ update: { ...(st.update ?? emptyUpdate()), checking: true, error: null } });
  try {
    const u = await check();
    if (!u) {
      pending = null;
      st.set({ update: { ...emptyUpdate(), checkedAt: Date.now() } });
      if (manual) st.notify("최신 버전입니다", "ok");
      return;
    }
    pending = u;
    st.set({ update: { ...emptyUpdate(), available: true, version: u.version, notes: u.body ?? "", date: u.date ?? null, checkedAt: Date.now() } });
    if (manual) st.notify(`새 버전 v${u.version} 사용 가능`, "info");
  } catch (e) {
    pending = null;
    st.set({ update: { ...emptyUpdate(), checkedAt: Date.now(), error: `${e}` } });
    if (manual) st.notify(`업데이트 확인 실패: ${e}`, "err");
  }
}

/** 내려받아 설치하고 앱을 재시작한다. */
export async function installUpdate(): Promise<void> {
  const st = useStore.getState();
  if (!pending) { await checkForUpdate(true); if (!pending) return; }
  const u = pending;
  let total = 0, done = 0;
  st.set({ update: { ...(st.update ?? emptyUpdate()), available: true, version: u.version, downloading: true, progress: 0 } });
  try {
    await u.downloadAndInstall((ev) => {
      if (ev.event === "Started") total = ev.data.contentLength ?? 0;
      else if (ev.event === "Progress") {
        done += ev.data.chunkLength;
        const p = total ? Math.min(99, Math.round((done / total) * 100)) : 0;
        useStore.getState().set({ update: { ...useStore.getState().update!, progress: p } });
      } else if (ev.event === "Finished") {
        useStore.getState().set({ update: { ...useStore.getState().update!, progress: 100 } });
      }
    });
    st.notify("업데이트 설치 완료 — 재시작합니다", "ok");
    await relaunch();
  } catch (e) {
    useStore.getState().set({ update: { ...useStore.getState().update!, downloading: false, error: `${e}` } });
    useStore.getState().notify(`업데이트 실패: ${e}`, "err");
  }
}

export function dismissUpdate() {
  const st = useStore.getState();
  if (st.update) st.set({ update: { ...st.update, dismissed: true } });
}

export function emptyUpdate() {
  return { available: false, version: null as string | null, notes: "", date: null as string | null, checking: false, downloading: false, progress: 0, error: null as string | null, dismissed: false, checkedAt: 0 };
}
