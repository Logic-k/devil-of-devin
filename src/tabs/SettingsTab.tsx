import { useEffect, useState } from "react";
import { api } from "../lib/ipc";

export default function SettingsTab({ settings, onSaved }: { settings: any; onSaved: () => void }) {
  const [s, setS] = useState<any>(null);
  const [auth, setAuth] = useState("");
  const [hookPort, setHookPort] = useState<number | null>(null);
  const [msg, setMsg] = useState("");

  useEffect(() => { if (settings) setS({ ...settings }); }, [settings]);
  useEffect(() => { api.devinAuthStatus().then(setAuth).catch((e) => setAuth(`${e}`)); }, []);

  const save = async () => { await api.settingsSet(s); setMsg("저장됨"); onSaved(); };

  if (!s) return <div className="panel dim">로딩…</div>;

  return (
    <div className="panel" style={{ maxWidth: 720 }}>
      <div className="card">
        <h3>Devin CLI</h3>
        <div className="col">
          <label className="field">devin.exe 경로 (비우면 자동 탐색)
            <input value={s.devin_path}
              onChange={(e) => setS({ ...s, devin_path: e.target.value })} />
          </label>
          <label className="field">기본 모델
            <input value={s.default_model}
              onChange={(e) => setS({ ...s, default_model: e.target.value })} />
          </label>
          <label className="field">기본 권한 모드
            <select value={s.default_permission_mode}
              onChange={(e) => setS({ ...s, default_permission_mode: e.target.value })}>
              {["normal", "accept-edits", "smart", "dangerous"].map((m) =>
                <option key={m} value={m}>{m}</option>)}
            </select>
          </label>
          <div className="small dim">인증 상태: <code>{auth || "…"}</code></div>
        </div>
      </div>

      <div className="card">
        <h3>훅 수집 서버 (워커 이벤트 탭)</h3>
        <div className="row">
          {hookPort ? (
            <>
              <span className="badge ok">127.0.0.1:{hookPort} 수신중</span>
              <button className="danger" onClick={() => api.hookServerStop().then(() => setHookPort(null))}>중지</button>
            </>
          ) : (
            <button onClick={() => api.hookServerStart().then(setHookPort)}>수신 서버 시작</button>
          )}
          {hookPort && (
            <button onClick={async () => {
              const p = await api.hooksInject("D:\\Devil_of_Devin", hookPort);
              setMsg(`hooks 주입 → ${p}`);
            }}>현재 프로젝트에 hooks 주입</button>
          )}
        </div>
        <div className="small dim" style={{ marginTop: 6 }}>
          .devin/hooks.v1.json 에 curl POST 훅을 추가해 워커의 PreToolUse/PostToolUse/Stop 등
          이벤트를 실시간 수집합니다.
        </div>
      </div>

      <div className="card">
        <h3>스킬 / GitHub</h3>
        <label className="field">추가 스킬 디렉터리 (쉼표 구분)
          <input value={(s.skills_dirs ?? []).join(",")}
            onChange={(e) => setS({ ...s, skills_dirs: e.target.value.split(",").map((x: string) => x.trim()).filter(Boolean) })} />
        </label>
        <label className="field">GitHub 토큰 (code search용, 선택)
          <input type="password" value={s.github_token ?? ""}
            onChange={(e) => setS({ ...s, github_token: e.target.value || null })} />
        </label>
      </div>

      <div className="card">
        <h3>자동화 — 워커 MCP/ENV 주입</h3>
        <div className="small dim">
          모든 워커의 `session/new`에 주입되는 MCP 서버 (ACP mcpServers 형식)와
          spawn 환경변수. Vercel/Supabase 배포 자동화에 사용.
        </div>
        <div className="row" style={{ margin: "8px 0" }}>
          <button className="small" onClick={() => {
            const cur = Array.isArray(s.mcp_servers) ? [...s.mcp_servers] : [];
            if (!cur.some((m: any) => m.name === "supabase")) {
              cur.push({ name: "supabase", url: "https://mcp.supabase.com/mcp" });
              setS({ ...s, mcp_servers: cur });
            }
          }}>+ Supabase MCP</button>
          <button className="small" onClick={() => {
            const cur = Array.isArray(s.mcp_servers) ? [...s.mcp_servers] : [];
            if (!cur.some((m: any) => m.name === "vercel")) {
              cur.push({ name: "vercel", url: "https://mcp.vercel.com" });
              setS({ ...s, mcp_servers: cur });
            }
          }}>+ Vercel MCP</button>
        </div>
        <label className="field">mcp_servers (JSON 배열)
          <textarea rows={5} className="mono"
            value={JSON.stringify(s.mcp_servers ?? [], null, 2)}
            onChange={(e) => {
              try { setS({ ...s, mcp_servers: JSON.parse(e.target.value) }); } catch {}
            }} />
        </label>
        <label className="field">worker_env (KEY=VALUE, 줄당 하나)
          <textarea rows={4} className="mono"
            value={Object.entries(s.worker_env ?? {}).map(([k, v]) => `${k}=${v}`).join("\n")}
            onChange={(e) => {
              const env: Record<string, string> = {};
              for (const line of e.target.value.split("\n")) {
                const i = line.indexOf("=");
                if (i > 0) env[line.slice(0, i).trim()] = line.slice(i + 1).trim();
              }
              setS({ ...s, worker_env: env });
            }} />
        </label>
        <div className="small dim">
          값에 <code>$secret:NAME</code> 을 쓰면 Windows 자격증명 관리자에서 읽습니다 —
          토큰을 이 파일에 평문으로 저장하지 않아도 됩니다.
        </div>
      </div>

      <SecretVault />

      <div className="row">
        <button className="primary" onClick={save}>설정 저장</button>
        <span className="small dim">{msg}</span>
      </div>
    </div>
  );
}

function SecretVault() {
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [msg, setMsg] = useState("");
  const [exists, setExists] = useState<boolean | null>(null);

  return (
    <div className="card">
      <h3>시크릿 보관소 (Windows 자격증명 관리자)</h3>
      <div className="small dim">
        API 토큰 등을 OS 보안 저장소에 보관. worker_env / MCP 설정에서
        <code>$secret:NAME</code>으로 참조합니다.
      </div>
      <div className="row" style={{ marginTop: 8 }}>
        <input placeholder="이름 (예: VERCEL_TOKEN)" value={name}
          onChange={(e) => { setName(e.target.value); setExists(null); }} />
        <input className="grow" type="password" placeholder="값" value={value}
          onChange={(e) => setValue(e.target.value)} />
        <button className="primary" disabled={!name.trim() || !value}
          onClick={async () => {
            await api.secretSet(name.trim(), value);
            setValue(""); setMsg(`저장됨: ${name.trim()}`); setExists(true);
          }}>저장</button>
        <button disabled={!name.trim()}
          onClick={async () => setExists(await api.secretExists(name.trim()))}>
          확인
        </button>
        <button className="danger" disabled={!name.trim()}
          onClick={async () => {
            await api.secretDelete(name.trim()); setExists(false); setMsg(`삭제됨: ${name.trim()}`);
          }}>삭제</button>
      </div>
      <div className="small dim" style={{ marginTop: 6 }}>
        {exists === true && <span className="badge ok">존재함</span>}
        {exists === false && <span className="badge warn">없음</span>} {msg}
      </div>
    </div>
  );
}
