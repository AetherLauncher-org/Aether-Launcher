// Picks the port `tauri dev` serves the front end on.
//
// The project's port is 1420. On Windows the system can reserve ranges of ports (Hyper-V and
// WinNAT take new ones at every boot) and a port inside one cannot be bound even though nothing
// listens on it: `listen EACCES`. `netsh int ipv4 show excludedportrange protocol=tcp` lists them.
// The port is kept when it can be bound and only moves to the next free one when it cannot.

import net from "node:net";

export const defaultPort = 1420;

/** Tries to bind `port` on `host`; resolves to `true` when that works. */
export function canBind(port, host) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.listen({ port, host, exclusive: true }, () => {
      server.close(() => resolve(true));
    });
  });
}

/** Both loopback families: Vite binds `localhost`, which can be either. */
export async function isFree(port) {
  return (await canBind(port, "127.0.0.1")) && (await canBind(port, "::1"));
}

/**
 * The first port from `start` up to `start + tries - 1` for which `free(port)` and
 * `free(port + 1)` hold (Vite's websocket for a phone on the network uses the next one).
 * Throws when there is none.
 */
export async function findPort(start = defaultPort, tries = 1000, free = isFree) {
  // Windows reserves ports in blocks of 100 and the blocks sit next to each other, so a wide
  // window is needed: here 1363-1662 was one stretch of reserved ports.
  for (let port = start; port < start + tries && port < 65535; port += 1) {
    if ((await free(port)) && (await free(port + 1))) {
      return port;
    }
  }
  throw new Error(`no free port from ${start} to ${start + tries - 1}`);
}
