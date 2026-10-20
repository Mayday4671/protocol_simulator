// Local UI inspection only. Reads the existing fixture database; all edits stay
// in browser memory. No database writes, UDP sockets, or native dialogs.
const { createServer } = require("node:http");
const { readFileSync, existsSync } = require("node:fs");
const path = require("node:path");
const { DatabaseSync } = require("node:sqlite");

const root = path.resolve(__dirname, "..");
const dist = path.join(root, "dist");
const db = new DatabaseSync(
  path.join(root, "artifacts/device-workspace-validation.db"),
  { readOnly: true },
);
const data = {
  systems: db.prepare("SELECT * FROM protocol_systems ORDER BY id").all(),
  groups: db.prepare("SELECT * FROM protocol_groups ORDER BY id").all(),
  nodes: db
    .prepare(
      "SELECT * FROM nodes WHERE id IN (SELECT node_ref FROM terminal_profiles WHERE id BETWEEN 1 AND 18) ORDER BY id",
    )
    .all(),
  profiles: db
    .prepare(
      "SELECT * FROM terminal_profiles WHERE id BETWEEN 1 AND 18 ORDER BY id",
    )
    .all(),
  protocols: db
    .prepare(
      "SELECT p.*,s.name system_name,g.name group_name FROM protocols p LEFT JOIN protocol_systems s ON s.id=p.system_ref LEFT JOIN protocol_groups g ON g.id=p.group_ref ORDER BY p.id",
    )
    .all(),
  dictionaries: db
    .prepare(
      "SELECT d.*,s.name system_name,g.name group_name FROM dict_sets d LEFT JOIN protocol_groups g ON g.id=d.group_ref LEFT JOIN protocol_systems s ON s.id=g.system_ref ORDER BY d.id",
    )
    .all(),
};
for (const d of data.dictionaries)
  d.items = db
    .prepare("SELECT * FROM dict_items WHERE set_ref=? ORDER BY sort_order,id")
    .all(d.id);
db.close();

function installBridge(data) {
  const callbacks = new Map(),
    listeners = new Map();
  let next = 0;
  const clone = (value) => structuredClone(value);
  const save = (rows, value, id = value.id) => {
    const row = rows.find((r) => r.id === id);
    if (row) Object.assign(row, value);
    else rows.push({ ...value, id: Math.max(0, ...rows.map((r) => r.id)) + 1 });
    return clone(row || rows.at(-1));
  };
  window.__uiInspection = { calls: [], data };
  window.__TAURI_INTERNALS__ = {
    transformCallback: (fn) => {
      callbacks.set(++next, fn);
      return next;
    },
    unregisterCallback: (id) => callbacks.delete(id),
    invoke: async (cmd, args = {}) => {
      window.__uiInspection.calls.push({ cmd, args: clone(args) });
      if (cmd === "plugin:event|listen") {
        listeners.set(++next, args);
        return next;
      }
      if (cmd === "plugin:event|unlisten")
        return listeners.delete(args.eventId);
      const key = {
        list_nodes: "nodes",
        list_systems: "systems",
        list_groups: "groups",
        list_protocols: "protocols",
        list_dict_sets: "dictionaries",
        list_terminals: "profiles",
      }[cmd];
      if (key) return clone(data[key]);
      if (cmd === "engine_status")
        return {
          listening_nodes: [],
          running_heartbeats: [],
          periodic_sends: [],
        };
      if (cmd === "log_stats") return { today_in: 0, today_out: 0, total: 0 };
      if (["query_logs", "list_heartbeats", "dict_set_refs"].includes(cmd))
        return [];
      if (cmd === "upsert_protocol") {
        const p = args.payload;
        return save(
          data.protocols,
          {
            ...p,
            fields_json: JSON.stringify(p.fields),
            system_name: data.systems.find((s) => s.id === p.system_ref)?.name,
            group_name: data.groups.find((g) => g.id === p.group_ref)?.name,
          },
          data.protocols.find((r) => r.protocol_id === p.protocol_id)?.id,
        );
      }
      if (cmd === "save_terminal")
        return save(data.profiles, {
          ...args.payload,
          session_hex:
            data.profiles.find((p) => p.id === args.payload.id)?.session_hex ??
            null,
        });
      if (cmd === "update_group_header")
        return save(
          data.groups,
          {
            header_enabled: args.payload.enabled,
            header_fields_json: JSON.stringify(args.payload.fields),
          },
          args.id,
        );
      if (cmd === "upsert_dict_set")
        return save(
          data.dictionaries,
          {
            ...args.payload,
            items: data.dictionaries.find((d) => d.id === args.id)?.items ?? [],
          },
          args.id,
        );
      if (cmd === "upsert_dict_item")
        return save(
          data.dictionaries.find((d) => d.id === args.payload.set_ref).items,
          args.payload,
          args.id,
        );
      if (cmd === "create_node" || cmd === "update_node")
        return save(
          data.nodes,
          {
            ...args.payload,
            group_bind_json: JSON.stringify(args.payload.group_binds),
          },
          args.id,
        );
      if (cmd === "create_system" || cmd === "update_system")
        return save(data.systems, args.payload, args.id);
      if (cmd === "create_group" || cmd === "update_group")
        return save(data.groups, args.payload, args.id);
      if (cmd === "plugin:dialog|open" || cmd === "plugin:dialog|save")
        return null;
      throw new Error("页面检查不执行此操作：" + cmd);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (id) => listeners.delete(id),
  };
}

const bridge = `(${installBridge.toString()})(${JSON.stringify(data).replaceAll("<", "\\u003c")});`;
const mime = {
  ".js": "text/javascript",
  ".css": "text/css",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
};
createServer((req, res) => {
  const url = new URL(req.url, "http://127.0.0.1");
  if (url.pathname === "/__ui_bridge.js") {
    res.writeHead(200, {
      "Content-Type": "text/javascript; charset=utf-8",
      "Cache-Control": "no-store",
    });
    return res.end(bridge);
  }
  const relative = decodeURIComponent(url.pathname).replace(/^\/+/, "");
  const file = path.resolve(dist, relative);
  if (!file.startsWith(dist + path.sep) && file !== dist) {
    res.writeHead(403);
    return res.end();
  }
  if (path.extname(file) && existsSync(file)) {
    res.writeHead(200, {
      "Content-Type": mime[path.extname(file)] || "application/octet-stream",
      "Cache-Control": "no-store",
    });
    return res.end(readFileSync(file));
  }
  const html = readFileSync(path.join(dist, "index.html"), "utf8").replace(
    "<head>",
    '<head><script src="/__ui_bridge.js"></script>',
  );
  res.writeHead(200, {
    "Content-Type": "text/html; charset=utf-8",
    "Cache-Control": "no-store",
  });
  res.end(html);
}).listen(1423, "127.0.0.1", () =>
  console.log(
    "UI inspection: http://127.0.0.1:1423 (memory-only edits, no UDP)",
  ),
);
