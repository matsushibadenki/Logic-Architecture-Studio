import { useEffect, useRef, useState } from "react";
import { rpc } from "./store";
import type { Snapshot } from "./types";

type Paths = Record<string, { points: [number, number][]; blocked: boolean }>;
type Drag = { ids: string[]; revision: number; dx: number; dy: number } | null;
// Rust owns routing in both renderers. Coalesce rapid pointer moves: at most
// one request in flight, then process only the latest preview. Stale replies
// never attach wires to a different revision or pointer position.
export function useRoutes(
  snapshot: Snapshot,
  drag: Drag,
  definition?: string,
): Paths {
  const [result, setResult] = useState<{ key: string; paths: Paths } | null>(
    null,
  );
  const queue = useRef<{ key: string; args: Record<string, unknown> } | null>(
    null,
  );
  const running = useRef(false);
  const identity = useRef({ document: snapshot.document, epoch: 0 });
  if (identity.current.document !== snapshot.document)
    identity.current = {
      document: snapshot.document,
      epoch: identity.current.epoch + 1,
    };
  const key = JSON.stringify([
    identity.current.epoch,
    snapshot.revision,
    drag,
    definition,
  ]);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      queue.current = null;
    };
  }, []);
  const latest = useRef(key);
  latest.current = key;
  useEffect(() => {
    let active = true;
    const positions =
      drag?.revision === snapshot.revision
        ? snapshot.document.nodes
            .filter((n) => drag.ids.includes(n.id))
            .map((n) => ({ id: n.id, x: n.x + drag.dx, y: n.y + drag.dy }))
        : [];
    queue.current = {
      key,
      args: { expected_revision: snapshot.revision, positions, definition },
    };
    const run = async () => {
      if (running.current) return;
      running.current = true;
      try {
        while (queue.current) {
          const request = queue.current;
          queue.current = null;
          try {
            const reply = await rpc<{ revision: number; routes: Paths }>(
              "view.routes",
              request.args,
            );
            if (mounted.current && latest.current === request.key)
              setResult({ key: request.key, paths: reply.routes });
          } catch {
            // Revision conflicts are retried by the next document update.
          }
        }
      } finally {
        running.current = false;
      }
    };
    const frame = requestAnimationFrame(() => {
      if (active) void run();
    });
    return () => {
      active = false;
      cancelAnimationFrame(frame);
    };
  }, [snapshot.document, snapshot.revision, key, definition]);
  return result?.key === key ? result.paths : {};
}
