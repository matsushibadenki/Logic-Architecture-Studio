import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import net from "node:net";
import path from "node:path";
import os from "node:os";
import fs from "node:fs";
import { createHash } from "node:crypto";
function coreSocket() {
  const root = process.env.LAS_WORKSPACE || process.cwd();
  const direct = path.join(root, ".logic/studio.sock");
  if (Buffer.byteLength(direct) <= 100) return direct;
  const digest = createHash("sha256")
    .update(fs.realpathSync(root))
    .digest("hex")
    .slice(0, 16);
  return path.join(os.tmpdir(), `las-${digest}`, "core.sock");
}
export default defineConfig({
  plugins: [
    react(),
    {
      name: "rust-design-core",
      configureServer(server) {
        server.middlewares.use("/api/command", (req, res) => {
          if (req.method !== "POST") {
            res.statusCode = 405;
            res.end();
            return;
          }
          // Same-origin only: the dev server is a local bridge, never a public API.
          if (
            req.headers.origin &&
            req.headers.origin !== `http://${req.headers.host}`
          ) {
            res.statusCode = 403;
            res.end();
            return;
          }
          let body = "";
          req.on("data", (chunk) => {
            body += chunk;
            if (body.length > 1048576) {
              req.destroy();
            }
          });
          req.on("end", () => {
            try {
              const request = JSON.parse(body);
              request.actor = "gui";
              const socket = net.createConnection(coreSocket());
              let response = "";
              socket.setTimeout(40000);
              socket.on("connect", () =>
                socket.write(JSON.stringify(request) + "\n"),
              );
              socket.on("data", (chunk) => {
                response += chunk;
                if (response.includes("\n")) {
                  res.setHeader("Content-Type", "application/json");
                  res.end(response);
                  socket.destroy();
                }
              });
              socket.on("timeout", () =>
                socket.destroy(new Error("Core request timed out")),
              );
              socket.on("error", () => {
                if (!res.writableEnded) {
                  res.statusCode = 503;
                  res.setHeader("Content-Type", "application/json");
                  res.end(
                    JSON.stringify({
                      ok: false,
                      error: "Design Core is offline. Run npm run core.",
                    }),
                  );
                }
              });
            } catch {
              res.statusCode = 400;
              res.end();
            }
          });
        });
      },
    },
  ],
  server: { port: 1420, strictPort: true },
  clearScreen: false,
});
