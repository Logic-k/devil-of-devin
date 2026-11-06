import { useEffect, useRef, useState } from "react";
import { api } from "../lib/ipc";

// devin models list needs full CLI auth which the session token doesn't
// satisfy — fall back to the model ids found in the CLI binary.
const FALLBACK_MODELS = [
  "swe-2-max",
  "swe-2-max-lightning",
  "swe-2-high-lightning",
  "swe-2-low-lightning",
  "cognition-swe-1.7",
  "claude-opus-4.6",
  "claude-sonnet-4.6",
  "claude-haiku-4.5",
  "gpt-5.6",
  "codex",
  "glm-5-3-flash-max",
];

let modelCache: string[] | null = null;
let modelFetch: Promise<string[]> | null = null;

function collectIds(v: any, out: Set<string>) {
  if (typeof v === "string" && /^[a-zA-Z0-9][a-zA-Z0-9._-]{2,}$/.test(v)) out.add(v);
  else if (Array.isArray(v)) v.forEach((x) => collectIds(x, out));
  else if (v && typeof v === "object") Object.values(v).forEach((x) => collectIds(x, out));
}

async function fetchModels(): Promise<string[]> {
  if (modelCache) return modelCache;
  if (!modelFetch) {
    modelFetch = (async () => {
      try {
        const r = await api.devinModels();
        const ids = new Set<string>();
        collectIds(r, ids);
        const list = [...ids].filter((s) => !s.includes("://")).slice(0, 60);
        modelCache = list.length ? [...new Set([...list, ...FALLBACK_MODELS.slice(0, 3)])] : FALLBACK_MODELS;
      } catch {
        modelCache = FALLBACK_MODELS;
      }
      return modelCache;
    })();
  }
  return modelFetch;
}

/** Text input with a native folder-picker button. */
export function DirInput({
  value, onChange, placeholder, style,
}: {
  value: string; onChange: (v: string) => void; placeholder?: string; style?: React.CSSProperties;
}) {
  const pick = async () => {
    try {
      const p = await api.pickFolder(value || undefined);
      if (p) onChange(p);
    } catch (e) { console.warn("pickFolder", e); }
  };
  return (
    <div className="dir-input" style={style}>
      <input value={value} placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)} />
      <button type="button" className="small icon" title="폴더 선택" onClick={pick}>📁</button>
    </div>
  );
}

/** Model input with a datalist of Devin models (CLI list + fallback). */
export function ModelInput({
  value, onChange, placeholder, style,
}: {
  value: string; onChange: (v: string) => void; placeholder?: string; style?: React.CSSProperties;
}) {
  const [models, setModels] = useState<string[]>(modelCache ?? FALLBACK_MODELS);
  const listId = useRef(`models-${Math.random().toString(36).slice(2, 8)}`);
  useEffect(() => { fetchModels().then(setModels).catch(() => {}); }, []);
  return (
    <>
      <input list={listId.current} value={value} placeholder={placeholder} style={style}
        onChange={(e) => onChange(e.target.value)} />
      <datalist id={listId.current}>
        {models.map((m) => <option key={m} value={m} />)}
      </datalist>
    </>
  );
}
