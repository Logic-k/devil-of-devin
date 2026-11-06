import { useStore } from "../store";
import { installUpdate, dismissUpdate } from "../lib/updater";

/** 새 버전이 있을 때 상단에 띄우는 띠. 설치 중에는 진행률을 보여준다. */
export default function UpdateBanner() {
  const u = useStore((s) => s.update);
  if (!u || !u.available || u.dismissed) return null;
  return (
    <div className="update-banner">
      <span>
        🔔 새 버전 <b>v{u.version}</b>이 있습니다.
        {u.downloading ? ` 내려받는 중… ${u.progress}%` : " 지금 설치하면 앱이 재시작됩니다."}
        {u.error && <span className="err"> 실패: {u.error}</span>}
      </span>
      <span className="row">
        <button className="small primary" disabled={u.downloading} onClick={installUpdate}>
          {u.downloading ? "설치 중…" : "업데이트 설치"}
        </button>
        {!u.downloading && <button className="small" onClick={dismissUpdate}>나중에</button>}
      </span>
    </div>
  );
}
