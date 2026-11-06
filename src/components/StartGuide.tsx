import { useState } from "react";

const HIDE_KEY = "dod.startGuide.hidden";

interface Props {
  doneCount: number;
  hasResearch: boolean;
  onResearch: () => void;
  onStage: (id: string) => void;
}

/**
 * 처음 쓰는 사람을 위한 시작 가이드. 산출물이 하나도 없는 프로젝트에서는 기본으로 펼쳐지고,
 * 접으면 localStorage 에 기억한다. "가이드" 버튼으로 언제든 다시 펼칠 수 있다.
 */
export default function StartGuide({ doneCount, hasResearch, onResearch, onStage }: Props) {
  const [hidden, setHidden] = useState<boolean>(() => {
    try { return localStorage.getItem(HIDE_KEY) === "1"; } catch { return false; }
  });
  const open = !hidden && doneCount === 0;
  const [forced, setForced] = useState(false);
  const show = open || forced;

  const hide = () => {
    try { localStorage.setItem(HIDE_KEY, "1"); } catch {}
    setHidden(true); setForced(false);
  };
  const reopen = () => {
    try { localStorage.removeItem(HIDE_KEY); } catch {}
    setHidden(false); setForced(true);
  };

  if (!show) {
    return (
      <div className="row small dim" style={{ justifyContent: "flex-end", padding: "0 4px" }}>
        <button className="tiny" onClick={reopen}>📖 사용 순서 가이드</button>
      </div>
    );
  }

  return (
    <div className="card guide">
      <h3>
        사용 순서 — 사람은 "무엇을 만들지"를 문서로 확정하고, 코드는 Loop가 자율로 짭니다
        <button className="tiny" style={{ marginLeft: "auto" }} onClick={hide}>접기</button>
      </h3>
      <ol className="guide-steps">
        <li>
          <b>리서치</b> <span className="dim">(선택, IDEA 전)</span> — 주제를 "문제 + 기존 해법 + 기술 선택지"로 적습니다.
          결과는 <code>research/</code>에 남고 IDEA·TRD 단계에 자동 참조됩니다.
          {hasResearch ? <span className="badge ok">완료</span> : <button className="tiny" onClick={onResearch}>리서치 실행</button>}
        </li>
        <li>
          <b>IDEA</b> — 만들고 싶은 것을 한 문단으로 적고 실행합니다. 워커가 질문하면 워커 탭의 coordinator 대화에서 답합니다.
          <button className="tiny" onClick={() => onStage("idea")}>IDEA 선택</button>
        </li>
        <li>
          <b>PRD</b> <span className="warn">가장 중요</span> — 요구마다 관찰 가능한 수용 기준, 그리고 <b>자율운영 합의</b>(권한·예산·금지·모델)를 꼭 채웁니다.
          Loop는 이 합의를 근거로 질문 없이 돕니다.
        </li>
        <li>
          <b>TRD → 설계</b> — 기술 비교가 필요하면 여기서 리서치를 한 번 더 돌려도 됩니다 ("A vs B, 우리 PRD 조건에서").
        </li>
        <li>
          <b>계획 → 하네스 → 루프 설정</b> — 계획 뒤 루프 보드에 태스크 카드가 보여야 하고, 하네스 뒤 게이트 4칸에 실행 명령이 들어와야 합니다.
          명령이 없으면 리뷰 에이전트가 판정해 느리고 비쌉니다.
        </li>
        <li>
          <b>Loop 실행</b> — 루프 보드에서 예산을 짧게(30분) 잡고 한 phase만 먼저 돌려봅니다. 실패 태스크는 <code>.devil/loop/&lt;id&gt;/</code>의
          시도 기록·검증 보고서를 보고 수용 기준을 고친 뒤 재개합니다.
        </li>
      </ol>
    </div>
  );
}
