import { useStore } from "../store";
import { runSelfcheck } from "../lib/events";

/** 설정 → "Devin 호환성": 검증 버전 대비 상태와 자가 점검 결과. */
export default function CompatCard() {
  const info = useStore((s) => s.info);
  const compat = useStore((s) => s.compat);
  const running = useStore((s) => s.compatRunning);
  const status: string = info?.version_status ?? "unknown";
  const installed = String(info?.version ?? "").split(" (")[0] || "불명";
  const statusLabel =
    status === "tested" ? <span className="badge ok">검증된 버전</span>
    : status === "newer" ? <span className="badge warn">검증 버전보다 새 버전 — 미검증</span>
    : status === "older" ? <span className="badge warn">검증 버전보다 오래된 버전</span>
    : <span className="badge">버전 불명</span>;

  return (
    <div className="card">
      <h3>Devin 호환성 {statusLabel}</h3>
      <div className="small">
        설치: <code>{installed}</code> · 검증: <code>{info?.tested_version ?? "3000.11.234"}</code>
        <span className="dim"> — 앱은 Devin 의 비공식 동작(세션 토큰 인증, <code>--agent-type review</code>, 세션 모드 이름)에 기대므로 버전이 다르면 아래 점검으로 확인합니다.</span>
      </div>
      <div className="row" style={{ marginTop: 8, alignItems: "center" }}>
        <button className="small" disabled={running} onClick={() => runSelfcheck(true)}>{running ? "점검 중… (devin 프로세스 2개 실행)" : "다시 점검"}</button>
        {compat && (
          <span className={`badge ${compat.ok ? "ok" : "err"}`}>
            {compat.ok ? "핵심 항목 통과" : `핵심 항목 실패 ${compat.checks.filter((c: any) => c.critical && c.ok === false).length}건`}
          </span>
        )}
        {compat?.ranAt && <span className="small dim">마지막 점검 {String(compat.ranAt).replace("T", " ").slice(0, 19)}</span>}
      </div>
      {compat ? (
        <div className="compat-list">
          {compat.checks.map((c: any) => (
            <div key={c.id} className={`compat-row ${c.ok === true ? "ok" : c.ok === false ? "fail" : "skip"}`}>
              <span className="mark">{c.ok === true ? "✓" : c.ok === false ? "✗" : "–"}</span>
              <span>{c.name}{c.critical && <span className="dim"> (핵심)</span>}</span>
              <span className="detail">{c.detail}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="small dim" style={{ marginTop: 6 }}>{running ? "점검 중…" : "아직 점검하지 않았습니다. 앱 시작 3초 뒤 자동으로 한 번 실행됩니다."}</div>
      )}
      <div className="small dim" style={{ marginTop: 8 }}>
        인증 폴백 순서: credentials.toml 세션 토큰 → ACP 브라우저 로그인 → 상단 "로그인" 버튼(<code>devin auth login</code>). 토큰이 거부되면 상단에 배너로 알립니다.
      </div>
    </div>
  );
}
