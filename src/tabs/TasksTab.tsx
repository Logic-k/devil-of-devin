import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { DirInput, ModelInput } from "../components/fields";

interface Task {
  id: string; title: string; prompt: string;
  dependsOn: string[]; status: "todo" | "running" | "done" | "error";
  workerId?: string; note?: string;
}

export default function TasksTab() {
  const [dir, setDir] = useState("D:\\Devil_of_Devin");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [draft, setDraft] = useState({ title: "", prompt: "", dependsOn: "" });
  const [running, setRunning] = useState(false);
  const [model, setModel] = useState("");
  const [autoApprove, setAutoApprove] = useState(true);

  const persist = (list: Task[]) => { setTasks(list); api.tasksSave(dir, list).catch(() => {}); };
  useEffect(() => { api.tasksLoad(dir).then(setTasks).catch(() => setTasks([])); }, [dir]);

  const add = () => {
    if (!draft.title.trim()) return;
    persist([...tasks, {
      id: Math.random().toString(36).slice(2, 10),
      title: draft.title.trim(), prompt: draft.prompt,
      dependsOn: draft.dependsOn.split(",").map((s) => s.trim()).filter(Boolean),
      status: "todo",
    }]);
    setDraft({ title: "", prompt: "", dependsOn: "" });
  };

  const ready = (t: Task) =>
    t.status === "todo" && t.dependsOn.every((d) => tasks.find((x) => x.title === d || x.id === d)?.status === "done");

  const runReady = async () => {
    setRunning(true);
    let list = [...tasks];
    for (;;) {
      const next = list.find((t) => t.status === "todo" &&
        t.dependsOn.every((d) => list.find((x) => x.title === d || x.id === d)?.status === "done"));
      if (!next) break;
      list = list.map((t) => t.id === next.id ? { ...t, status: "running" } : t);
      persist(list);
      try {
        const w = await api.workerCreate({
          name: `task-${next.title}`, cwd: dir, model: model || undefined,
          autoApprove,
        });
        await api.workerPrompt(w.id, next.prompt);
        list = list.map((t) => t.id === next.id ? { ...t, status: "done", workerId: w.id } : t);
      } catch (e) {
        list = list.map((t) => t.id === next.id ? { ...t, status: "error", note: `${e}` } : t);
      }
      persist(list);
    }
    setRunning(false);
  };

  const cols: [Task["status"], string][] = [["todo", "대기"], ["running", "실행중"], ["done", "완료"], ["error", "오류"]];

  return (
    <div className="panel">
      <div className="card">
        <h3>태스크 보드 — 프로젝트 <code>{dir}</code></h3>
        <div className="row">
          <DirInput style={{ width: 380 }} value={dir} onChange={setDir} />
          <ModelInput style={{ width: 180 }} placeholder="모델(기본 swe-2-max)" value={model}
            onChange={setModel} />
          <label className="row small dim">
            <input type="checkbox" checked={autoApprove} onChange={(e) => setAutoApprove(e.target.checked)} />
            자동승인
          </label>
          <button className="primary" onClick={runReady} disabled={running || !tasks.some(ready)}>
            준비된 태스크 실행 (DAG 순서)
          </button>
        </div>
      </div>

      <div className="card">
        <h3>새 태스크</h3>
        <div className="row">
          <input placeholder="제목" value={draft.title}
            onChange={(e) => setDraft({ ...draft, title: e.target.value })} />
          <input className="grow" placeholder="프롬프트" value={draft.prompt}
            onChange={(e) => setDraft({ ...draft, prompt: e.target.value })} />
          <input placeholder="의존 (제목, 쉼표)" value={draft.dependsOn}
            onChange={(e) => setDraft({ ...draft, dependsOn: e.target.value })} />
          <button onClick={add} disabled={!draft.title.trim()}>추가</button>
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 12 }}>
        {cols.map(([status, label]) => (
          <div key={status} className="card" style={{ minHeight: 200 }}>
            <h3>{label} ({tasks.filter((t) => t.status === status).length})</h3>
            {tasks.filter((t) => t.status === status).map((t) => (
              <div key={t.id} className="worker-item">
                <div className="name">{t.title}</div>
                <div className="meta">{t.prompt.slice(0, 80)}</div>
                {!!t.dependsOn.length && <div className="meta">depends: {t.dependsOn.join(", ")}</div>}
                {t.note && <div className="meta" style={{ color: "var(--err)" }}>{t.note}</div>}
                <button className="small danger" style={{ marginTop: 6 }}
                  onClick={() => persist(tasks.filter((x) => x.id !== t.id))}>삭제</button>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
