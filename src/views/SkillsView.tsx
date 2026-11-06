import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { useStore } from "../store";

export default function SkillsView() {
  const notify = useStore((s) => s.notify);
  const settings = useStore((s) => s.settings);
  const [harness, setHarness] = useState<any>(null);
  const [gitRef, setGitRef] = useState("");
  const [local, setLocal] = useState<any[]>([]);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<any[]>([]);
  const [src, setSrc] = useState("");
  const [destRoot, setDestRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");

  const scan = () => api.skillsScanLocal().then(setLocal).catch(() => {});
  const loadHarness = () => api.harnessStatus().then(setHarness).catch(() => {});
  useEffect(() => { scan(); loadHarness(); setGitRef(settings?.harness_ref ?? ""); }, [settings?.harness_ref]);

  const installHarness = async () => {
    setBusy(true);
    try { setHarness(await api.harnessInstall(gitRef || undefined)); notify("하네스 스킬 팩 설치 완료", "ok"); scan(); }
    catch (e) { notify(`설치 실패: ${e}`, "err"); }
    finally { setBusy(false); }
  };

  const search = async () => {
    setBusy(true); setMsg("");
    try {
      const r = await api.skillsGithubSearch(query);
      const items = r?.body?.items ?? [];
      setResults(items.map((it: any) => ({
        name: it.name, repo: it.repository?.full_name ?? it.full_name,
        path: it.path, html: it.html_url, desc: it.repository?.description ?? it.description,
      })));
      if (r?.fallback) setMsg("(code search 제한 — 저장소 검색으로 폴백. 설정의 시크릿 GITHUB_TOKEN 을 넣으면 코드 검색이 됩니다)");
    } catch (e) { setMsg(`검색 실패: ${e}`); }
    setBusy(false);
  };

  const install = async (source: string) => {
    setBusy(true); setMsg("");
    try { const dest = await api.skillsInstall(source, destRoot || ""); setMsg(`설치됨 → ${dest}`); scan(); }
    catch (e) { setMsg(`설치 실패: ${e}`); }
    setBusy(false);
  };

  const harnessNames = new Set<string>(["idea-brainstorm", "prd-generator", "trd-generator", "architecture-generator", "plan-generator", "harness-setup", "harness-update", "loop-setup", "loop-update", "code-to-docs", "feature-add", "bug-fix", "refactor-plan"]);

  return (
    <div className="panel">
      <div className="card">
        <h3>하네스 루프 스킬 팩 <span className={`badge ${harness?.complete ? "ok" : "warn"}`}>{harness ? `${harness.installed.length}/13` : "…"}</span></h3>
        <p className="small dim">
          <code>GangJaeYu/harness_loop_full_korean</code> — IDEA → PRD → TRD → 설계 → 계획 → 하네스 → 루프를 이끄는 한국어 스킬 13개.
          Devin 사용자 스킬 폴더 <code className="mono">{harness?.skillsDir}</code> 에 설치되어 모든 세션에서 <code>/스킬명</code>으로 호출됩니다.
        </p>
        <div className="row">
          <input style={{ width: 200 }} placeholder="git ref (예: v2.2.4, 비우면 최신)" value={gitRef} onChange={(e) => setGitRef(e.target.value)} />
          <button className="primary" disabled={busy} onClick={installHarness}>{harness?.complete ? "다시 설치 / 업데이트" : "설치"}</button>
          {harness?.version && <span className="small dim">설치됨: v{harness.version} ({harness.commit}) · {harness.installedAt}</span>}
        </div>
        {harness && harness.missing.length > 0 && <div className="small dim" style={{ marginTop: 6 }}>누락: {harness.missing.join(", ")}</div>}
      </div>

      <div className="card">
        <h3>내 컴퓨터의 스킬 <button className="tiny" onClick={scan}>다시 검사</button></h3>
        {local.map((s) => (
          <div key={s.dir} className={`skill-row${s.enabled ? "" : " dim"}`}>
            <span className="badge">{s.source}</span>
            {harnessNames.has(s.name) && <span className="badge ok">harness</span>}
            {!s.enabled && <span className="badge warn">비활성</span>}
            <strong>{s.name}</strong>
            <span className="desc">{s.description || s.dir}</span>
            <button className="tiny" onClick={() => api.skillsSetEnabled(s.dir, !s.enabled).then(scan)}>{s.enabled ? "비활성화" : "활성화"}</button>
          </div>
        ))}
        {!local.length && <div className="dim small">발견된 스킬 없음</div>}
      </div>

      <div className="card">
        <h3>GitHub에서 스킬 찾기 / 설치</h3>
        <div className="row">
          <input className="grow" placeholder="검색어 (예: code review, aws, docs…)" value={query}
            onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Enter" && search()} />
          <button onClick={search} disabled={busy || !query.trim()}>검색</button>
        </div>
        <div className="row" style={{ marginTop: 8 }}>
          <input className="grow" placeholder="설치: owner/repo[#경로] 또는 GitHub URL" value={src} onChange={(e) => setSrc(e.target.value)} />
          <input style={{ width: 300 }} placeholder="설치 대상 루트 (비우면 %APPDATA%\devin\skills)" value={destRoot} onChange={(e) => setDestRoot(e.target.value)} />
          <button className="primary" disabled={busy || !src.trim()} onClick={() => install(src)}>설치</button>
        </div>
        {msg && <div className="small dim" style={{ marginTop: 6 }}>{msg}</div>}
        <div style={{ marginTop: 8 }}>
          {results.map((r, i) => (
            <div key={i} className="skill-row">
              <div className="grow"><strong>{r.repo}</strong><span className="dim small"> / {r.path}</span><div className="desc">{r.desc}</div></div>
              <button className="tiny" onClick={() => install(`${r.repo}#${r.path?.replace(/\/SKILL\.md$/i, "")}`)}>설치</button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
