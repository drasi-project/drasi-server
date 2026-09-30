import { memo } from "react";
import type { Node, NodeProps } from "@xyflow/react";
import { Boxes } from "lucide-react";
import type { ComputationComponent } from "@/api/types";
import { computationColor, computationStatus } from "@/utils/computation";
import StatusBadge from "@/components/shared/StatusBadge";
import NodeShell from "./NodeShell";

type ComputationFlowNode = Node<{
  inspection: ComputationComponent;
  internal: boolean;
  locked?: boolean;
}>;

export default memo(function ComputationNode({ data, id }: NodeProps<ComputationFlowNode>) {
  const component = data.inspection;
  const color = computationColor(component.role);
  return (
    <NodeShell
      nodeId={id}
      cardClass="node-card"
      accentClass="text-drasi-text-secondary"
      collapsedWidth={220}
      expandedWidth={220}
      collapsedHeight={122}
      status={computationStatus(component)}
      expanded={false}
      canToggle={false}
      locked={data.locked}
      handles="both"
      handleClass="!bg-drasi-text-secondary"
      header={
        <>
          <Boxes size={18} style={{ color }} className="shrink-0" />
          <div className="min-w-0">
            <div className="text-xs font-semibold text-drasi-text-primary truncate" title={component.id}>
              {component.id}
            </div>
            <div className="text-[10px] uppercase" style={{ color }}>
              {component.role}{data.internal ? " (internal)" : ""}
            </div>
          </div>
        </>
      }
    >
      <StatusBadge status={computationStatus(component)} />
    </NodeShell>
  );
})
