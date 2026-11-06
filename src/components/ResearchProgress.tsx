import { useEffect, useState } from "react";

/** 리서치 런의 단계 목록을 spec 으로 계산한다 (백엔드 steps_total 과 같은 순서). */
function stepList(run: any): { key: string; label: string }[] {
  const spec = run.spec ?? {};
  const rounds = Math.max(1, spec.reviewRounds ?? 1);
  const refine = Math.min(spec.refineRounds ?? 0, rounds);
  const steps: { key: string; label: string }[] = [{ key: "plan", label: "계획" }, { key: "sweep", label: "수집" }];
  for (let r = 1; r <= rounds; r++) {
    steps.push({ key: `review-${r}`, label: `${r}차 검토` });
    if (r <= refine) steps.push({ key: `refine-${r}`, label: `${r}차 보강` });
  }
  if (spec.synthesize) steps.push({ key: "synth", label: "종합" });
  return steps;
}

const PHASE_KO: Record<string, string> = { queued: "대기", plan: "계획", sweep: "수집", synth: "종합", done: "완료", error: "오류", stopped: "중단됨" };
function phaseLabel(p: string) {
  if (PHASE_KO[p]) return PHASE_KO[p];
  const m = p.match(/^(review|refine)-(\d+)$/);
  return m ? `${m[2]}차 ${m[1] === "review" ? "검토" : "보강"}` : p;
}

function fmtDur(ms: number) {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}초`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}분 ${s % 60}초` : `${Math.floor(m / 60)}시간 ${m % 60}분`;
}

/** 리서치 실시간 현황: 진행률, 회차, 지금 하는 일, 남은 단계, 예상 남은 시간. */
export default function ResearchProgress({ run, compact = false }: { run: any; compact?: boolean }) {
  const [, tick] = useState(0);
  useEffect(() => {
    if (run?.done) return;
    const t = setInterval(() => tick((n) => n + 1), 5000);
    return () => clearInterval(t);
  }, [run?.done]);
  if (!run) return null;

  const total = Math.max(1, run.stepsTotal ?? 1);
  const done = Math.min(total, run.stepsDone ?? 0);
  const pct = run.done && !run.error && run.phase !== "stopped" ? 100 : Math.round((done / total) * 100);
  const started = run.startedAt ? Date.parse(run.startedAt) : NaN;
  const elapsed = isNaN(started) ? 0 : Date.now() - started;
  const eta = !run.done && done > 0 ? (elapsed / done) * (total - done) : null;
  const steps = stepList(run);
  const nowIdx = steps.findIndex((s) => s.key === run.phase);
  const remaining = run.done ? [] : steps.slice(nowIdx + 1);

  if (compact) {
    return (
      <span className="rp-compact" title={run.current || phaseLabel(run.phase)}>
        <span className="rp-bar"><div style={{ width: `${pct}%` }} /></span>
        <span>{pct}% · {phaseLabel(run.phase)}{run.rounds_total > 1 || run.roundsTotal > 1 ? ` (${run.round}/${run.roundsTotal})` : ""}</span>
        {eta != null && <span>· 약 {fmtDur(eta)} 남음</span>}
      </span>
    );
  }

  return (
    <div className="rp">
      <div className="row small" style={{ justifyContent: "space-between" }}>
        <span>
          <b>{run.done ? (run.error ? "오류" : run.phase === "stopped" ? "중단됨" : "완료") : phaseLabel(run.phase)}</b>
          {!run.done && run.roundsTotal > 1 && run.round > 0 && <span className="dim"> · 검토 {run.round}/{run.roundsTotal}회차</span>}
          {run.phase === "sweep" && <span className="dim"> · 질문 {run.sweepDone}/{run.sweepTotal}</span>}
          <span className="dim"> · 단계 {done}/{total}</span>
        </span>
        <span className="dim">
          경과 {fmtDur(elapsed)}
          {eta != null && ` · 예상 남은 시간 약 ${fmtDur(eta)}`}
          {!run.done && done === 0 && " · 첫 단계가 끝나면 예상 시간을 계산합니다"}
        </span>
      </div>
      <div className={`rp-bar ${run.done ? (run.error || run.phase === "stopped" ? "err" : "done") : ""}`}><div style={{ width: `${pct}%` }} /></div>
      {run.current && !run.done && <div className="small">⏳ {run.current}</div>}
      <div className="rp-steps">
        {steps.map((s, i) => (
          <span key={s.key} className={`rp-step ${run.done && !run.error && run.phase !== "stopped" ? "done" : i < nowIdx ? "done" : i === nowIdx ? "now" : ""}`}>
            {i < nowIdx || (run.done && !run.error && run.phase !== "stopped") ? "✓ " : i === nowIdx ? "▶ " : ""}{s.label}
          </span>
        ))}
      </div>
      {remaining.length > 0 && (
        <div className="small dim">남은 단계: {remaining.map((s) => s.label).join(" → ")}</div>
      )}
    </div>
  );
}
