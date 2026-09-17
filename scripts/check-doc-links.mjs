import { readdir, readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

const root = process.cwd();
const ignoredDirectories = new Set(['.git', 'node_modules', 'target', 'dist']);
const markdownLinkPattern = /!?\[[^\]]*\]\(([^)]+)\)/g;
const htmlLinkPattern = /\b(?:href|src)\s*=\s*["']([^"']+)["']/gi;

function stripCode(markdown) {
  return markdown
    .replace(/```[\s\S]*?```/g, '')
    .replace(/~~~[\s\S]*?~~~/g, '')
    .replace(/`[^`\n]*`/g, '');
}

function extractDestination(raw) {
  const value = raw.trim();
  if (!value) return '';

  if (value.startsWith('<')) {
    const end = value.indexOf('>');
    return end === -1 ? value.slice(1) : value.slice(1, end);
  }

  const match = value.match(/^(\S+)/);
  return match?.[1] ?? '';
}

function isExternal(destination) {
  return (
    destination.startsWith('#') ||
    destination.startsWith('//') ||
    /^[a-z][a-z0-9+.-]*:/i.test(destination) ||
    destination.includes('${{')
  );
}

function resolveLocalTarget(sourceFile, destination) {
  const withoutFragment = destination.split('#', 1)[0].split('?', 1)[0];
  if (!withoutFragment) return null;

  let decoded;
  try {
    decoded = decodeURI(withoutFragment);
  } catch {
    decoded = withoutFragment;
  }

  const sourceDir = path.dirname(sourceFile);
  return decoded.startsWith('/')
    ? path.join(root, decoded.slice(1))
    : path.resolve(sourceDir, decoded);
}

async function collectMarkdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    if (entry.name.startsWith('.') && entry.name !== '.github') {
      continue;
    }

    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (!ignoredDirectories.has(entry.name)) {
        files.push(...(await collectMarkdownFiles(fullPath)));
      }
    } else if (entry.isFile() && entry.name.endsWith('.md')) {
      files.push(fullPath);
    }
  }

  return files;
}

async function targetExists(target) {
  try {
    await stat(target);
    return true;
  } catch {
    return false;
  }
}

const markdownFiles = await collectMarkdownFiles(root);
const failures = [];
let checkedLinks = 0;

for (const file of markdownFiles) {
  const markdown = stripCode(await readFile(file, 'utf8'));
  const destinations = [];

  for (const match of markdown.matchAll(markdownLinkPattern)) {
    destinations.push(extractDestination(match[1]));
  }
  for (const match of markdown.matchAll(htmlLinkPattern)) {
    destinations.push(match[1].trim());
  }

  for (const destination of destinations) {
    if (!destination || isExternal(destination)) continue;

    const target = resolveLocalTarget(file, destination);
    if (!target) continue;

    checkedLinks += 1;
    if (!(await targetExists(target))) {
      failures.push({
        source: path.relative(root, file),
        destination,
        resolved: path.relative(root, target),
      });
    }
  }
}

if (failures.length > 0) {
  console.error(`Found ${failures.length} broken local documentation link(s):`);
  for (const failure of failures) {
    console.error(`- ${failure.source}: ${failure.destination} -> ${failure.resolved}`);
  }
  process.exit(1);
}

console.log(
  `Checked ${checkedLinks} local link(s) across ${markdownFiles.length} Markdown file(s).`,
);
