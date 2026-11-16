import { wireWidth } from "../nodeAppearance";
import type { Kind } from "../types";
// Same normalized outlines as unge-render/src/gates.rs. Port geometry is unchanged.
export function isGate(kind: Kind) {
  return ["and", "or", "xor", "not", "nand", "nor", "xnor"].includes(kind);
}
export function GateSymbol({
  kind,
  height,
  tiny,
  light,
  zoom,
}: {
  kind: Kind;
  height: number;
  tiny: boolean;
  light: boolean;
  zoom: number;
}) {
  const inverted = ["not", "nand", "nor", "xnor"].includes(kind);
  const disjunction = ["or", "nor", "xor", "xnor"].includes(kind);
  const path =
    kind === "not"
      ? "M12 10 L82 50 L12 90 Z"
      : disjunction
        ? "M12 10 Q58 10 82 50 Q58 90 12 90 Q44 50 12 10"
        : "M12 10 H46 A36 40 0 0 1 46 90 H12 Z";
  const size = tiny ? 195.84 : 100;
  const ratio = size / 100;
  const stroke = wireWidth(zoom);
  const left = (288 - size) / 2;
  const top = (height - size) / 2;
  return (
    <g
      className={tiny ? "node-gate-overview" : "node-gate"}
      fill="none"
      stroke={tiny ? "white" : "var(--node-tone)"}
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {(!tiny || light) && (
        <>
          {(kind === "not" ? [50] : [25, 75]).map((y, i) => {
            const anchor = kind === "not" ? height / 2 : (height * (i + 1)) / 3;
            const t = (y - 10) / 80;
            const x = disjunction ? 12 + 64 * t * (1 - t) : 12;
            return (
              <path
                key={y}
                d={`M0 ${anchor} H${left - 12} V${top + y * ratio} H${left + x * ratio}`}
              />
            );
          })}
          <path
            d={`M${left + (inverted ? 94 : 82) * ratio} ${height / 2} H288`}
          />
        </>
      )}
      <g
        transform={`translate(${left},${top}) scale(${ratio})`}
        strokeWidth={stroke / ratio}
      >
        <path d={path} />
        {["xor", "xnor"].includes(kind) && <path d="M3 10 Q35 50 3 90" />}
        {inverted && <circle cx="88" cy="50" r="6" />}
      </g>
    </g>
  );
}
