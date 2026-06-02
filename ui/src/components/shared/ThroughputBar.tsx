import { getTypeColor, type ComponentType } from "@/utils/colors";

interface ThroughputBarProps {
  value: number;
  max: number;
  type: ComponentType;
}

export default function ThroughputBar({
  value,
  max,
  type,
}: ThroughputBarProps) {
  const pct = Math.min((value / Math.max(max, 1)) * 100, 100);
  const color = getTypeColor(type);

  return (
    <div className="throughput-bar">
      <div
        className="throughput-bar-fill"
        style={{
          width: `${pct}%`,
          backgroundColor: color,
          boxShadow: `0 0 6px ${color}40`,
        }}
      />
    </div>
  );
}
