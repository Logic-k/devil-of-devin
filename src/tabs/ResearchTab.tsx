import { useEffect, useRef, useState } from "react";
import { api, ev } from "../lib/ipc";

export default function ResearchTab({ defaultModel }: { defaultModel?: string }) {
  const [spec, setSpec] = useState({
    topic: "",
    targetDir: "D:\\Devil_of_Devin",
    researchers: 4,
    questionsPerResearcher: 3,
    reviewRounds: 2,
    refineRounds: 1,
    synthesize: true,
    model: "",
    autoApprove: true,
  });
  const [runId, setRunId] = useState<string | null>(null);
  const [run, setRun] = useState<any>(null);
  const [log, setLog] = useState<string[]>([]);
  const logEnd = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!runId) return;
    const un = ev.researchProgress(runId, (p: any) => {
      setLog((l) => [...l, p.msg]);
      api.researchStatus(runId).then(setRun).catch(() => {});
    });
    const t = setInterval(() => api.researchStatus(runId).then(setRun).catch(() => {}), 3000);
    return () => { un.then((f) => f()); clearInterval(t); };
  }, [runId]);

  useEffect(() => { logEnd.current?.scrollIntoView({ behavior: "smooth" }); }, [log]);

  const start = async () => {
    setLog([]);
    const id = await api.researchStart({
      topic: spec.topic, targetDir: spec.targetDir,
      researchers: spec.researchers, questionsPerResearcher: spec.questionsPerResearcher,
      reviewRounds: spec.reviewRounds, refineRounds: spec.refineRounds,
      synthesize: spec.synthesize, model: spec.model || undefined,
      autoApprove: spec.autoApprove,
    });
    setRunId(id);
  };

  const num = (k: keyof typeof spec, label: string, min = 0, max = 10) => (
    <label className="field">
      {label}
      <input type="number" min={min} max={max} value={spec[k] as number}
        onChange={(e) => setSpec({ ...spec, [k]: +e.target.value } as any)} />
    </label>
  );

  return (
    <div className="panel">
      <div className="grid2">
        <div className="card">
          <h3>리서치 파이프라인 (원클릭)</h3>
          <div className="col">
            <label className="field">주제
              <input value={spec.topic} placeholder="예: Tauri v2 auto-updater 모범 사례"
                onChange={(e) => setSpec({ ...spec, topic: e.target.value })} />
            </label>
            <label className="field">대상 프로젝트 디렉터리
              <input value={spec.targetDir}
                onChange={(e) => setSpec({ ...spec, targetDir: e.target.value })} />
            </label>
            <div className="row">
              {num("researchers", "리서처 수")}
              {num("questionsPerResearcher", "리서처당 질문")}
              {num("reviewRounds", "검토 라운드")}
              {num("refineRounds", "보강 라운드")}
            </div>
            <div className="row">
              <label className="field">모델
                <input value={spec.model} placeholder={defaultModel ?? "swe-2-max"}
                  onChange={(e) => setSpec({ ...spec, model: e.target.value })} />
              </label>
              <label className="row small dim" style={{ alignSelf: "end" }}>
                <input type="checkbox" checked={spec.autoApprove}
                  onChange={(e) => setSpec({ ...spec, autoApprove: e.target.checked })} />
                권한 자동 승인
              </label>
              <label className="row small dim" style={{ alignSelf: "end" }}>
                <input type="checkbox" checked={spec.synthesize}
                  onChange={(e) => setSpec({ ...spec, synthesize: e.target.checked })} />
                최종 종합 md 생성
              </label>
            </div>
            <button className="primary" onClick={start} disabled={!spec.topic.trim() || (run && !run.done && !run.error)}>
              리서치 시작
            </button>
          </div>
        </div>

        <div className="card">
          <h3>진행 상황 {run && <span className="badge run">{run.phase}</span>}
            {run?.done && <span className="badge ok">완료</span>}
            {run?.error && <span className="badge err">오류</span>}</h3>
          <div className="log-view">
            {log.map((l, i) => <div key={i} className="progress-phase mono">{l}</div>)}
            <div ref={logEnd} />
          </div>
          {run?.error && <div className="small" style={{ color: "var(--err)" }}>{run.error}</div>}
          {!!run?.artifacts?.length && (
            <div className="small">
              <div className="dim">산출물:</div>
              {run.artifacts.map((a: string) => <div key={a} className="mono">{a}</div>)}
            </div>
          )}
          <div className="small dim" style={{ marginTop: 8 }}>
            단계: plan → sweep(병렬) → review×{spec.reviewRounds} → refine×{spec.refineRounds} → synth.
            산출물은 대상 디렉터리의 <code>research/</code> 아래 생성됩니다.
          </div>
        </div>
      </div>
    </div>
  );
}
