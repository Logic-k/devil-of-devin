import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { useStore } from "../store";
import Markdown from "../components/Markdown";

function Tree({ entries, sel, onOpen, depth = 0 }: any) {
  return (
    <>
      {entries.map((e: any) => <TreeNode key={e.rel} e={e} sel={sel} onOpen={onOpen} depth={depth} />)}
    </>
  );
}

function TreeNode({ e, sel, onOpen, depth }: any) {
  const [open, setOpen] = useState<boolean>(depth < 1 || e.rel.startsWith("docs") || e.rel.startsWith("plan_setup"));
  if (e.isDir) {
    return (
      <div>
        <div className="tree-row dir" style={{ paddingLeft: 8 + depth * 12 }} onClick={() => setOpen((o) => !o)}>
          <span className="tree-caret">{open ? "▾" : "▸"}</span> {e.name}
        </div>
        {open && <Tree entries={e.children} sel={sel} onOpen={onOpen} depth={depth + 1} />}
      </div>
    );
  }
  return (
    <div className={`tree-row file ${sel === e.rel ? "selected" : ""}`} style={{ paddingLeft: 22 + depth * 12 }}
      onClick={() => onOpen(e.rel)} title={e.modified ?? ""}>
      {e.name}
    </div>
  );
}

export default function DocsView() {
  const project = useStore((s) => s.activeProject());
  const notify = useStore((s) => s.notify);
  const [tree, setTree] = useState<any[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [text, setText] = useState<string>("");
  const [edit, setEdit] = useState(false);
  const [draft, setDraft] = useState("");
  const [raw, setRaw] = useState(false);

  const refreshTree = async () => {
    if (!project) return;
    try { setTree(await api.projectDocs(project.path)); } catch {}
  };
  const open = async (rel: string) => {
    if (!project) return;
    setSel(rel); setEdit(false);
    try { setText(await api.projectFileRead(project.path, rel)); } catch (e) { setText(`읽기 실패: ${e}`); }
  };
  useEffect(() => {
    refreshTree();
    const t = setInterval(refreshTree, 8000);
    return () => clearInterval(t);
  }, [project?.path]);
  const pendingDoc = useStore((s) => s.pendingDoc);
  useEffect(() => {
    if (!pendingDoc) return;
    useStore.setState({ pendingDoc: null });
    open(pendingDoc);
  }, [pendingDoc, project?.path]);

  const save = async () => {
    if (!project || !sel) return;
    try { await api.projectFileWrite(project.path, sel, draft); setText(draft); setEdit(false); notify("저장됨", "ok"); }
    catch (e) { notify(`${e}`, "err"); }
  };

  if (!project) return <div className="panel center"><div className="dim">프로젝트를 선택하세요</div></div>;

  return (
    <div className="docs">
      <div className="docs-tree">
        <div className="row small" style={{ padding: "6px 8px" }}>
          <b>문서</b><button className="tiny ghost" style={{ marginLeft: "auto" }} onClick={refreshTree}>↻</button>
        </div>
        {tree.length === 0 && <div className="side-empty">docs/·plan_setup/·harness_setup/ 등이 아직 없습니다</div>}
        <Tree entries={tree} sel={sel} onOpen={open} />
      </div>
      <div className="docs-body">
        {sel ? (
          <>
            <div className="docs-head row small">
              <span className="mono">{sel}</span>
              <div className="row" style={{ marginLeft: "auto" }}>
                <button className="tiny" onClick={() => open(sel)}>다시 읽기</button>
                <button className={`tiny ${raw ? "active" : ""}`} onClick={() => setRaw((r) => !r)}>{raw ? "렌더" : "원문"}</button>
                {!edit && <button className="tiny" onClick={() => { setDraft(text); setEdit(true); }}>편집</button>}
                {edit && <button className="tiny primary" onClick={save}>저장</button>}
                {edit && <button className="tiny" onClick={() => setEdit(false)}>취소</button>}
              </div>
            </div>
            <div className="docs-content">
              {edit ? (
                <textarea className="mono editor" value={draft} onChange={(e) => setDraft(e.target.value)} />
              ) : raw || !sel.endsWith(".md") ? (
                <pre className="small">{text}</pre>
              ) : (
                <Markdown text={text} />
              )}
            </div>
          </>
        ) : (
          <div className="center dim">왼쪽에서 문서를 선택하세요</div>
        )}
      </div>
    </div>
  );
}
