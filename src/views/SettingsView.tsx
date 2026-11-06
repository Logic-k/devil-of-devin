import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { useStore } from "../store";
import { refreshInfo } from "../lib/events";
import { ModelInput } from "../components/fields";
import UpdateCard from "../components/UpdateCard";
import CompatCard from "../components/CompatCard";

const MODES = ["smart", "accept-edits", "bypass", "plan", "ask"];

export default function SettingsView() {
  const settings = useStore((s) => s.settings);
  const project = useStore((s) => s.activeProject());
  const notify = useStore((s) => s.notify);
  const [s, setS] = useState<any>(null);
  const [auth, setAuth] = useState("");
  const [hookPort, setHookPort] = useState<number | null>(null);

  useEffect(() => { if (settings) setS({ ...settings }); }, [settings]);
  useEffect(() => {
    api.devinAuthStatus().then(setAuth).catch((e) => setAuth(`${e}`));
    api.hookServerStatus().then(setHookPort).catch(() => {});
  }, []);

  const save = async () => {
    try { await api.settingsSet(s); await refreshInfo(); notify("설정 저장됨", "ok"); }
    catch (e) { notify(`${e}`, "err"); }
  };

  if (!s) return <div className="panel dim">로딩…</div>;
  const f = (k: string, label: string, props: any = {}) => (
    <label className="field">{label}<input value={s[k] ?? ""} onChange={(e) => setS({ ...s, [k]: props.type === "number" ? +e.target.value : e.target.value })} {...props} /></label>
  );

  return (
    <div className="panel" style={{ maxWidth: 860 }}>
      <div className="card">
        <h3>Devin CLI</h3>
        <div className="grid2">
          {f("devin_path", "devin.exe 경로 (비우면 자동 탐색)")}
          <label className="field">기본 모델<ModelInput value={s.default_model} onChange={(v) => setS({ ...s, default_model: v })} /></label>
          <label className="field">기본 세션 모드
            <select value={s.default_permission_mode} onChange={(e) => setS({ ...s, default_permission_mode: e.target.value })}>
              {MODES.map((m) => <option key={m} value={m}>{m}</option>)}
            </select>
          </label>
        </div>
        <div className="small dim" style={{ marginTop: 6 }}>
          smart = 모델이 안전하다고 판단한 도구 자동 승인 · accept-edits = 읽기+편집 자동 · bypass = 전부 자동(주의, Windows는 샌드박스 없음). CLI 인증 상태: <code>{auth || "…"}</code>
        </div>
      </div>

      <div className="card">
        <h3>Loop 엔진</h3>
        <div className="grid3">
          {f("loop_concurrency", "동시 워커 수", { type: "number", min: 1, max: 8 })}
          {f("loop_heavy_concurrency", "무거운 게이트(e2e) 동시 수", { type: "number", min: 1, max: 4 })}
          {f("loop_max_attempts", "태스크당 최대 시도 (×2 = 전략 전환 포함)", { type: "number", min: 1, max: 10 })}
          {f("loop_gate_timeout_secs", "게이트 명령 타임아웃(초)", { type: "number", min: 60 })}
          <label className="field">구현 워커 모델<ModelInput value={s.loop_implementer_model ?? ""} placeholder={s.default_model} onChange={(v) => setS({ ...s, loop_implementer_model: v })} /></label>
          <label className="field">검증 워커 모델<ModelInput value={s.loop_validator_model ?? ""} placeholder={s.default_model} onChange={(v) => setS({ ...s, loop_validator_model: v })} /></label>
          <label className="field">Loop 세션 모드
            <select value={s.loop_mode ?? "smart"} onChange={(e) => setS({ ...s, loop_mode: e.target.value })}>
              {MODES.map((m) => <option key={m} value={m}>{m}</option>)}
            </select>
          </label>
          {f("harness_ref", "하네스 스킬 팩 git ref (비우면 최신)")}
        </div>
        <label className="row small dim" style={{ marginTop: 8 }}>
          <input type="checkbox" checked={!!s.loop_use_worktrees} onChange={(e) => setS({ ...s, loop_use_worktrees: e.target.checked })} />
          git 워크트리로 태스크 격리 (.devil/worktrees/&lt;task&gt;, 브랜치 loop/&lt;task&gt;; 통과 시 --no-ff 병합)
        </label>
      </div>

      <div className="card">
        <h3>훅 수집 서버</h3>
        <div className="row">
          {hookPort ? (
            <>
              <span className="badge ok">127.0.0.1:{hookPort} 수신 중</span>
              <button className="danger small" onClick={() => api.hookServerStop().then(() => setHookPort(null))}>중지</button>
              {project && <button className="small" onClick={async () => { const p = await api.hooksInject(project.path, hookPort); notify(`hooks 주입 → ${p}`, "ok"); }}>현재 프로젝트에 hooks 주입</button>}
            </>
          ) : (
            <button className="small" onClick={() => api.hookServerStart().then(setHookPort)}>수신 서버 시작</button>
          )}
          <label className="row small dim"><input type="checkbox" checked={!!s.auto_install_hooks} onChange={(e) => setS({ ...s, auto_install_hooks: e.target.checked })} /> 앱 시작 시 자동 기동 + 워커 생성 시 자동 주입</label>
        </div>
        <div className="small dim" style={{ marginTop: 6 }}>워커의 PreToolUse/PostToolUse/Stop 이벤트가 상단 "활동" 패널에 표시됩니다.</div>
      </div>

      <div className="card">
        <h3>스킬 디렉터리</h3>
        <label className="field">추가 스캔 디렉터리 (쉼표 구분)
          <input value={(s.skills_dirs ?? []).join(",")} onChange={(e) => setS({ ...s, skills_dirs: e.target.value.split(",").map((x: string) => x.trim()).filter(Boolean) })} />
        </label>
      </div>

      <div className="card">
        <h3>워커 MCP / ENV 주입</h3>
        <div className="small dim">모든 워커 세션의 <code>session/new</code>에 주입되는 MCP 서버와 spawn 환경변수. 값에 <code>$secret:NAME</code>을 쓰면 시크릿 보관소에서 읽습니다 (MCP headers 포함).</div>
        <div className="row" style={{ margin: "8px 0" }}>
          {[["supabase", "https://mcp.supabase.com/mcp"], ["vercel", "https://mcp.vercel.com"]].map(([name, url]) => (
            <button key={name} className="small" onClick={() => {
              const cur = Array.isArray(s.mcp_servers) ? [...s.mcp_servers] : [];
              if (!cur.some((m: any) => m.name === name)) setS({ ...s, mcp_servers: [...cur, { name, url }] });
            }}>+ {name} MCP</button>
          ))}
        </div>
        <label className="field">mcp_servers (JSON 배열)
          <textarea rows={5} className="mono" defaultValue={JSON.stringify(s.mcp_servers ?? [], null, 2)}
            onBlur={(e) => { try { setS({ ...s, mcp_servers: JSON.parse(e.target.value) }); } catch { notify("mcp_servers JSON 파싱 실패", "err"); } }} />
        </label>
        <label className="field">worker_env (KEY=VALUE, 줄당 하나)
          <textarea rows={4} className="mono" defaultValue={Object.entries(s.worker_env ?? {}).map(([k, v]) => `${k}=${v}`).join("\n")}
            onBlur={(e) => {
              const env: Record<string, string> = {};
              for (const line of e.target.value.split("\n")) { const i = line.indexOf("="); if (i > 0) env[line.slice(0, i).trim()] = line.slice(i + 1).trim(); }
              setS({ ...s, worker_env: env });
            }} />
        </label>
        <div className="small dim">유용한 변수: DEVIN_REFUSAL_FALLBACK=swe-2-max,swe-2-flash · DEVIN_MODEL</div>
      </div>

      <CompatCard />

      <SecretVault />

      <UpdateCard />

      <div className="row sticky-save">
        <button className="primary" onClick={save}>설정 저장</button>
      </div>
    </div>
  );
}

function SecretVault() {
  const notify = useStore((s) => s.notify);
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [exists, setExists] = useState<boolean | null>(null);
  return (
    <div className="card">
      <h3>시크릿 보관소 (Windows 자격증명 관리자)</h3>
      <div className="small dim">API 토큰을 OS 보안 저장소에 보관합니다. <code>GITHUB_TOKEN</code>을 저장하면 스킬 코드 검색에 사용됩니다.</div>
      <div className="row" style={{ marginTop: 8 }}>
        <input placeholder="이름 (예: GITHUB_TOKEN, VERCEL_TOKEN)" value={name} onChange={(e) => { setName(e.target.value); setExists(null); }} />
        <input className="grow" type="password" placeholder="값" value={value} onChange={(e) => setValue(e.target.value)} />
        <button className="primary small" disabled={!name.trim() || !value} onClick={async () => { await api.secretSet(name.trim(), value); setValue(""); setExists(true); notify(`저장됨: ${name.trim()}`, "ok"); }}>저장</button>
        <button className="small" disabled={!name.trim()} onClick={async () => setExists(await api.secretExists(name.trim()))}>확인</button>
        <button className="danger small" disabled={!name.trim()} onClick={async () => { await api.secretDelete(name.trim()); setExists(false); notify(`삭제됨: ${name.trim()}`, "ok"); }}>삭제</button>
        {exists === true && <span className="badge ok">존재함</span>}
        {exists === false && <span className="badge warn">없음</span>}
      </div>
    </div>
  );
}
