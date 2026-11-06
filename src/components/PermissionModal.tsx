import { useStore, type PermReq } from "../store";
import { api } from "../lib/ipc";

export default function PermissionModal({ req }: { req: PermReq }) {
  const popPerm = useStore((s) => s.popPerm);
  const agents = useStore((s) => s.agents);
  const respond = async (outcome: string, optionId?: string) => {
    const result = outcome === "cancelled"
      ? { outcome: { outcome: "cancelled" } }
      : { outcome: { outcome: "selected", optionId } };
    try { await api.acpPermissionRespond(req.agentId, req.rpcId, result); } catch {}
    popPerm(req.id);
  };
  const tc = req.params?.toolCall;
  return (
    <div className="perm-overlay" onClick={() => respond("cancelled")}>
      <div className="perm-modal" onClick={(e) => e.stopPropagation()}>
        <h3>권한 요청 <span className="dim small">{agents[req.agentId]?.label ?? req.agentId}</span></h3>
        <div className="perm-title">{tc?.title ?? ""}</div>
        {tc?.kind && <span className="badge">{tc.kind}</span>}
        <pre className="small perm-body">
          {JSON.stringify(tc?.rawInput ?? tc ?? req.params, null, 2).slice(0, 2500)}
        </pre>
        <div className="perm-opts">
          {(req.params?.options ?? []).map((o: any) => (
            <button key={o.optionId ?? o.id}
              className={o.kind?.startsWith("allow") ? "primary" : ""}
              onClick={() => respond("selected", o.optionId ?? o.id)}>
              {o.name ?? o.kind ?? o.optionId}
            </button>
          ))}
          <button className="danger" onClick={() => respond("cancelled")}>거부</button>
        </div>
      </div>
    </div>
  );
}
