import { useEffect, useRef, useState } from "react";
import { api, ev } from "../lib/ipc";
import { DirInput, ModelInput } from "../components/fields";

export default function DevelopTab({ defaultModel }: { defaultModel?: string }) {
  const [spec, setSpec] = useState({
    targetDir: "D:\\Devil_of_Devin",
    goal: "",
    maxCycles: 3,
    verifyCommand: "",
    model: "",
    autoApprove: true,
  });
  const [runId, setRunId] = useState<string | null>(null);
  const [run, setRun] = useState<any>(null);
  const [log, setLog] = useState<string[]>([]);
  const logEnd = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!runId) return;
    const un = ev.developProgress(runId, (p: any) => {
      setLog((l) => [...l, p.msg]);
      api.developStatus(runId).then(setRun).catch(() => {});
    });
    const t = setInterval(() => api.developStatus(runId).then(setRun).catch(() => {}), 3000);
    return () => { un.then((f) => f()); clearInterval(t); };
  }, [runId]);
  useEffect(() => { logEnd.current?.scrollIntoView({ behavior: "smooth" }); }, [log]);

  const start = async () => {
    setLog([]);
    const id = await api.developStart({
      targetDir: spec.targetDir, goal: spec.goal,
      maxCycles: spec.maxCycles,
      verifyCommand: spec.verifyCommand || undefined,
      model: spec.model || undefined, autoApprove: spec.autoApprove,
    });
    setRunId(id);
  };

  return (
    <div className="panel">
      <div className="grid2">
        <div className="card">
          <h3>개발 파이프라인 (원클릭)</h3>
          <div className="col">
            <label className="field">대상 프로젝트 디렉터리
              <DirInput value={spec.targetDir}
                onChange={(v) => setSpec({ ...spec, targetDir: v })} />
            </label>
            <label className="field">목표 (비우면 research/에서 추론)
              <textarea value={spec.goal} rows={3}
                placeholder="예: 리서치 문서 기반으로 앱 전체 스캐폴딩을 완성"
                onChange={(e) => setSpec({ ...spec, goal: e.target.value })} />
            </label>
            <div className="row">
              <label className="field">구현↔검토 사이클
                <input type="number" min={1} max={8} value={spec.maxCycles}
                  onChange={(e) => setSpec({ ...spec, maxCycles: +e.target.value })} />
              </label>
              <label className="field grow">검증 명령 (선택)
                <input value={spec.verifyCommand} placeholder="예: npm run build"
                  onChange={(e) => setSpec({ ...spec, verifyCommand: e.target.value })} />
              </label>
            </div>
            <div className="row">
              <label className="field">모델
                <ModelInput value={spec.model} placeholder={defaultModel ?? "swe-2-max"}
                  onChange={(v) => setSpec({ ...spec, model: v })} />
              </label>
              <label className="row small dim" style={{ alignSelf: "end" }}>
                <input type="checkbox" checked={spec.autoApprove}
                  onChange={(e) => setSpec({ ...spec, autoApprove: e.target.checked })} />
                권한 자동 승인
              </label>
            </div>
            <button className="primary" onClick={start}
              disabled={run && !run.done && !run.error}>
              개발 시작
            </button>
          </div>
        </div>
        <div className="card">
          <h3>진행 상황 {run && <span className="badge run">{run.phase}</span>}
            {run?.done && !run.error && <span className="badge ok">완료</span>}
            {run?.error && <span className="badge err">오류</span>}</h3>
          <div className="log-view">
            {log.map((l, i) => <div key={i} className="progress-phase mono">{l}</div>)}
            <div ref={logEnd} />
          </div>
          {run?.error && <div className="small" style={{ color: "var(--err)" }}>{run.error}</div>}
          <div className="small dim" style={{ marginTop: 8 }}>
            단계: plan (research/*.md → .devil/plan.md) → implement ↔ review ×{spec.maxCycles}.
            리뷰어가 이슈 없음을 보고하면 조기 종료합니다.
          </div>
        </div>
      </div>
    </div>
  );
}
