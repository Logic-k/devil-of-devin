import { useEffect, useState } from "react";
import { useStore } from "../store";
import { checkForUpdate, installUpdate, currentVersion } from "../lib/updater";

/** 설정 화면의 "업데이트" 카드: 현재 버전, 수동 확인, 설치. */
export default function UpdateCard() {
  const u = useStore((s) => s.update);
  const [ver, setVer] = useState("…");
  useEffect(() => { currentVersion().then(setVer); }, []);
  return (
    <div className="card">
      <h3>업데이트</h3>
      <div className="row" style={{ alignItems: "center", gap: 10 }}>
        <span>현재 버전 <code>v{ver}</code></span>
        <button className="small" disabled={u?.checking || u?.downloading} onClick={() => checkForUpdate(true)}>
          {u?.checking ? "확인 중…" : "업데이트 확인"}
        </button>
        {u?.available && (
          <button className="small primary" disabled={u.downloading} onClick={installUpdate}>
            {u.downloading ? `설치 중… ${u.progress}%` : `v${u.version} 설치 후 재시작`}
          </button>
        )}
      </div>
      <div className="small dim" style={{ marginTop: 6 }}>
        {u?.error ? <span className="err">마지막 확인 실패: {u.error}</span>
          : u?.checkedAt ? (u.available ? `새 버전 v${u.version} 사용 가능` : "최신 버전입니다")
          : "시작 5초 뒤와 6시간마다 GitHub Release(latest.json)를 확인합니다. 개발 모드(tauri dev)에서는 자동 확인을 건너뜁니다."}
        {u?.available && u.notes ? <pre className="mono" style={{ marginTop: 6, whiteSpace: "pre-wrap" }}>{u.notes}</pre> : null}
      </div>
    </div>
  );
}
