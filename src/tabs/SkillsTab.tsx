import { useEffect, useState } from "react";
import { api } from "../lib/ipc";

export default function SkillsTab() {
  const [local, setLocal] = useState<any[]>([]);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<any[]>([]);
  const [src, setSrc] = useState("");
  const [destRoot, setDestRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");

  const scan = () => api.skillsScanLocal().then(setLocal).catch(() => {});
  useEffect(() => {
    scan();
    api.settingsGet().then((s) => {
      setDestRoot(s?.skills_dirs?.[0] ?? "");
    }).catch(() => {});
  }, []);

  const search = async () => {
    setBusy(true); setMsg("");
    try {
      const r = await api.skillsGithubSearch(query);
      const items = r?.body?.items ?? [];
      setResults(items.map((it: any) => ({
        name: it.name, repo: it.repository?.full_name ?? it.full_name,
        path: it.path, html: it.html_url, desc: it.repository?.description ?? it.description,
      })));
      if (r?.fallback) setMsg("(code search 제한 — 저장소 검색으로 폴백)");
    } catch (e) { setMsg(`검색 실패: ${e}`); }
    setBusy(false);
  };

  const install = async (source: string) => {
    setBusy(true); setMsg("");
    try {
      const dest = await api.skillsInstall(source, destRoot || "");
      setMsg(`설치됨 → ${dest}`);
      scan();
    } catch (e) { setMsg(`설치 실패: ${e}`); }
    setBusy(false);
  };

  return (
    <div className="panel">
      <div className="card">
        <h3>내 컴퓨터의 스킬 <button className="small" onClick={scan}>다시 검사</button></h3>
        {local.map((s) => (
          <div key={s.dir} className={`skill-row${s.enabled ? "" : " dim"}`}>
            <span className="badge">{s.source}</span>
            {!s.enabled && <span className="badge warn">비활성</span>}
            <strong>{s.name}</strong>
            <span className="desc">{s.description || s.dir}</span>
            <button className="small" onClick={() =>
              api.skillsSetEnabled(s.dir, !s.enabled).then(scan)}>
              {s.enabled ? "비활성화" : "활성화"}
            </button>
          </div>
        ))}
        {!local.length && <div className="dim small">발견된 스킬 없음 —
          설정에서 스킬 디렉터리를 추가할 수 있습니다.</div>}
      </div>

      <div className="card">
        <h3>GitHub에서 스킬 찾기 / 설치</h3>
        <div className="row">
          <input className="grow" placeholder="검색어 (예: code review, aws, docs…)"
            value={query} onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && search()} />
          <button onClick={search} disabled={busy || !query.trim()}>검색</button>
        </div>
        <div className="row" style={{ marginTop: 8 }}>
          <input className="grow" placeholder="설치: owner/repo[#경로] 또는 GitHub URL"
            value={src} onChange={(e) => setSrc(e.target.value)} />
          <input style={{ width: 320 }} placeholder="설치 대상 루트 (예: %APPDATA%\devin\skills)"
            value={destRoot} onChange={(e) => setDestRoot(e.target.value)} />
          <button className="primary" disabled={busy || !src.trim()}
            onClick={() => install(src)}>설치</button>
        </div>
        {msg && <div className="small dim" style={{ marginTop: 6 }}>{msg}</div>}
        <div style={{ marginTop: 8 }}>
          {results.map((r, i) => (
            <div key={i} className="skill-row">
              <div className="grow">
                <strong>{r.repo}</strong>
                <span className="dim small"> / {r.path}</span>
                <div className="desc">{r.desc}</div>
              </div>
              <button className="small" onClick={() =>
                install(`${r.repo}#${r.path?.replace(/\/SKILL\.md$/i, "")}`)}>설치</button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
