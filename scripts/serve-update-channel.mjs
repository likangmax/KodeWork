// Serves a generated release channel over loopback HTTP so the whole update path
// (manifest fetch -> artifact download -> signature and version verification) can
// be rehearsed without public DNS, TLS or the production bucket.
//
// Layout produced by scripts/publish-update.ps1:
//   <root>/stable/latest.json
//   <root>/stable/kodework-windows-<version>-x86_64.msi
//
// Usage: node scripts/serve-update-channel.mjs [--root <dir>] [--port 8787]
import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { extname, join, normalize, resolve, sep } from 'node:path';

const args = process.argv.slice(2);
function option(name, fallback) {
  const index = args.indexOf(name);
  return index === -1 ? fallback : args[index + 1];
}

const root = resolve(option('--root', 'release-channel'));
const port = Number.parseInt(option('--port', '8787'), 10);
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  console.error(`invalid --port: ${option('--port', '')}`);
  process.exit(2);
}

const TYPES = {
  '.json': 'application/json',
  '.msi': 'application/octet-stream',
  '.sig': 'text/plain; charset=utf-8',
};

const server = createServer((request, response) => {
  const requested = decodeURIComponent((request.url ?? '/').split('?')[0]);
  // Contain every request inside the channel root: `normalize` collapses `..`
  // before it can escape, and the prefix check catches what is left.
  const candidate = resolve(join(root, normalize(requested)));
  if (candidate !== root && !candidate.startsWith(root + sep)) {
    response.writeHead(403).end('forbidden');
    return;
  }
  let target = candidate;
  try {
    if (statSync(target).isDirectory()) target = join(target, 'index.html');
  } catch {
    response.writeHead(404).end('not found');
    return;
  }
  let size;
  try {
    const stats = statSync(target);
    if (!stats.isFile()) throw new Error('not a file');
    size = stats.size;
  } catch {
    response.writeHead(404).end('not found');
    return;
  }
  response.writeHead(200, {
    'content-type': TYPES[extname(target).toLowerCase()] ?? 'application/octet-stream',
    'content-length': size,
    'cache-control': 'no-store',
  });
  // A client that disconnects mid-download would otherwise raise EPIPE on the
  // response, and an unhandled 'error' event takes the whole server down: the
  // next request then fails to connect instead of reporting anything useful.
  const stream = createReadStream(target);
  stream.on('error', () => response.destroy());
  response.on('error', () => stream.destroy());
  response.on('close', () => stream.destroy());
  stream.pipe(response);
});

server.on('clientError', (_error, socket) => {
  if (socket.writable) socket.end('HTTP/1.1 400 Bad Request\r\n\r\n');
  socket.destroy();
});

server.listen(port, '127.0.0.1', () => {
  console.log(`update channel root : ${root}`);
  console.log(`manifest            : http://127.0.0.1:${port}/stable/latest.json`);
});
