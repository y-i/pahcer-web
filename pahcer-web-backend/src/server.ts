import { fileURLToPath } from 'node:url';
import { Hono } from 'hono';
import { serve } from '@hono/node-server';
import { serveStatic } from '@hono/node-server/serve-static';
import { dirname, join, resolve, relative } from 'node:path';
import { spawn, execSync } from 'node:child_process';

import { Storage, type GlobalConfig, type LocalConfig } from './storage.js';
import { streamText } from 'hono/streaming';
import { writeFile, readFile, mkdir, rm } from 'node:fs/promises';

function getPahcerListParsed(baseDir: string) {
    try {
        const output = execSync('pahcer list', { encoding: 'utf-8', cwd: baseDir });
        const lines = output.trim().split('\n');
        
        // Very basic parsing: first line header, others data
        const header = lines[0]?.split(/\s+/).filter(Boolean) || [];
        const rows = lines.slice(1).map(line => {
            const cols = line.split(/\s+/).filter(Boolean);
            const obj: Record<string, string> = {};
            header.forEach((h, i) => {
                obj[h] = cols[i] || '';
            });
            return obj;
        });
        return { raw: output, parsed: rows };
    } catch (e) {
        return { raw: '', parsed: [] };
    }
}

async function downloadRecursive(url: string, destDir: string) {
    const visited = new Set<string>();
    const queue: { url: string; relPath: string }[] = [{ url, relPath: 'index.html' }];

    // 同名の .js と _bg.wasm を明示的に追加（wasm-bindgen等で動的にロードされる場合があるため）
    try {
        const u = new URL(url);
        const path = u.pathname;
        if (path.endsWith('.html')) {
            const base = path.substring(path.lastIndexOf('/') + 1, path.lastIndexOf('.html'));
            const dirUrl = url.substring(0, url.lastIndexOf('/') + 1);
            queue.push({ url: dirUrl + base + '.js', relPath: base + '.js' });
            queue.push({ url: dirUrl + base + '_bg.wasm', relPath: base + '_bg.wasm' });
        }
    } catch (e) {
        console.error('Failed to parse base name for visualizer siblings:', e);
    }

    await mkdir(destDir, { recursive: true });

    while (queue.length > 0) {
        const { url: currentUrl, relPath } = queue.shift()!;
        if (visited.has(currentUrl)) continue;
        visited.add(currentUrl);

        try {
            const res = await fetch(currentUrl);
            if (!res.ok) continue;

            const contentType = res.headers.get('content-type') || '';
            const buffer = await res.arrayBuffer();
            const filePath = join(destDir, relPath);
            await mkdir(dirname(filePath), { recursive: true });
            await writeFile(filePath, Buffer.from(buffer));

            // HTMLまたはCSSをパースして追加のリソースを探す
            if (contentType.includes('text/html') || contentType.includes('text/css')) {
                const text = new TextDecoder().decode(buffer);
                // HTML用
                const resourceRegex = /(?:src|href|content)\s*=\s*["']([^"']+\.(?:js|css|wasm|png|jpg|svg|ico|json))["']/gi;
                // CSS用 (url(...))
                const cssResourceRegex = /url\(['"]?([^'"]+\.(?:png|jpg|svg|ico|wasm|woff2?))['"]?\)/gi;
                
                let match;
                while ((match = resourceRegex.exec(text)) !== null) {
                    const foundPath = match[1];
                    if (foundPath.startsWith('http') || foundPath.startsWith('//') || foundPath.startsWith('data:')) continue;
                    
                    const resourceUrl = new URL(foundPath, currentUrl).toString();
                    if (!visited.has(resourceUrl)) {
                        queue.push({ url: resourceUrl, relPath: foundPath.replace(/^\//, '') });
                    }
                }
                while ((match = cssResourceRegex.exec(text)) !== null) {
                    const foundPath = match[1];
                    if (foundPath.startsWith('http') || foundPath.startsWith('//') || foundPath.startsWith('data:')) continue;
                    
                    const resourceUrl = new URL(foundPath, currentUrl).toString();
                    if (!visited.has(resourceUrl)) {
                        queue.push({ url: resourceUrl, relPath: foundPath.replace(/^\//, '') });
                    }
                }
            }
        } catch (e) {
            console.error(`Failed to download ${currentUrl}:`, e);
        }
    }
}

export async function startServer(options: any) {
    const baseDir = resolve(options.directory || process.cwd());
    const storage = new Storage(baseDir);

    const __filename = fileURLToPath(import.meta.url);
    const __dirname = dirname(__filename);
    const backendRoot = resolve(__dirname, '..'); 
    const frontendDir = resolve(backendRoot, '../pahcer-web-frontend');
    const distDir = join(frontendDir, 'dist');
    
    if (options.build) {
      console.log(`Building frontend in ${frontendDir}...`);
      try {
        execSync('npm run build', { cwd: frontendDir, stdio: 'inherit' });
      } catch (e) {
        console.error("Failed to build frontend.");
        process.exit(1);
      }
    }

    const app = new Hono();
    const api = new Hono();

    // --- API Routes ---

    // Config
    api.get('/config', async (c) => {
      const global = await storage.getGlobalConfig();
      const local = await storage.getLocalConfig();
      return c.json({ global, local });
    });

    api.post('/config/global', async (c) => {
      const config = await c.req.json();
      await storage.saveGlobalConfig(config);
      return c.json({ success: true });
    });

    api.post('/config/local', async (c) => {
      const config = await c.req.json();
      await storage.saveLocalConfig(config);
      return c.json({ success: true });
    });

    // Jobs
    api.get('/jobs', async (c) => {
      const jobs = await storage.getJobs();
      return c.json(jobs);
    });

    // Visualizer
    api.get('/visualizer/status', async (c) => {
        const exists = await storage.hasVisualizer();
        return c.json({ exists });
    });

    api.post('/visualizer/download', async (c) => {
      const { url } = await c.req.json();
      if (!url) return c.json({ error: 'URL is required' }, 400);
      
      try {
        const destDir = storage.getVisualizerDir();
        // Clear existing
        try { await rm(destDir, { recursive: true, force: true }); } catch {}
        
        await downloadRecursive(url, destDir);
        return c.json({ success: true });
      } catch (e) {
        return c.json({ error: String(e) }, 500);
      }
    });

    // Analysis
    api.post('/analysis/download', async (c) => {
      const url = 'https://img.atcoder.jp/ahc_standings/index.html';
      try {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`Failed to fetch analysis tool: ${res.statusText}`);
        const html = await res.text();
        const path = storage.getAnalysisPath();
        await writeFile(path, html);
        return c.json({ success: true });
      } catch (e) {
        return c.json({ error: String(e) }, 500);
      }
    });

    // Run pahcer
    api.post('/run', async (c) => {
      const body = await c.req.json();
      const args = body.args || [];
      
      return streamText(c, async (stream) => {
        const child = spawn('pahcer', ['run', ...args], {
          stdio: ['ignore', 'pipe', 'pipe'],
          shell: true,
          cwd: baseDir
        });

        const jobId = Date.now().toString();
        const fullLogs: string[] = [];
        
        await storage.saveJob({
          id: jobId,
          datetime: new Date().toISOString(),
          command: 'run',
          args,
          status: 'running'
        });

        child.stdout.on('data', (data) => {
          const text = data.toString();
          fullLogs.push(text);
          stream.write(JSON.stringify({ type: 'stdout', data: text }) + '\n');
        });

        child.stderr.on('data', (data) => {
          const text = data.toString();
          fullLogs.push(text);
          stream.write(JSON.stringify({ type: 'stderr', data: text }) + '\n');
        });

        const exitCode = await new Promise<number>((resolve) => {
          child.on('close', (code) => resolve(code ?? 0));
        });

        const allOutput = fullLogs.join('');
        const scoreMatch = allOutput.match(/Score\s*=\s*([\d,]+)/i);
        const score = scoreMatch ? parseInt(scoreMatch[1].replace(/,/g, '')) : undefined;

        await storage.saveJob({
          id: jobId,
          datetime: new Date().toISOString(),
          command: 'run',
          args,
          status: exitCode === 0 ? 'success' : 'failed',
          result: {
            score,
            logs: allOutput
          }
        });

        stream.write(JSON.stringify({ type: 'exit', code: exitCode }) + '\n');
      });
    });

    // List results (wrapper for pahcer list)
    api.get('/list', async (c) => {
      const result = getPahcerListParsed(baseDir);
      return c.json(result);
    });

    app.route('/api', api);

    // Serve Visualizer Directory
    const visualizerDir = storage.getVisualizerDir();
    app.get('/visualizer/*', async (c) => {
        const relPath = c.req.path.replace('/visualizer/', '') || 'index.html';
        const filePath = join(visualizerDir, relPath);
        try {
            const content = await readFile(filePath);
            const contentType = relPath.endsWith('.js') ? 'application/javascript' : 
                               relPath.endsWith('.css') ? 'text/css' : 
                               relPath.endsWith('.wasm') ? 'application/wasm' :
                               relPath.endsWith('.html') ? 'text/html' :
                               relPath.endsWith('.png') ? 'image/png' :
                               relPath.endsWith('.jpg') ? 'image/jpeg' :
                               relPath.endsWith('.svg') ? 'image/svg+xml' :
                               'application/octet-stream';
            return c.body(content, 200, { 'Content-Type': contentType });
        } catch {
            return c.text('Not Found', 404);
        }
    });

    app.get('/visualizer.html', async (c) => {
        const path = storage.getVisualizerPath();
        try {
            const content = await readFile(path);
            return c.html(content.toString());
        } catch {
            return c.text('Visualizer not found. Please download it from settings.', 404);
        }
    });

    // Serve Analysis Tool
    app.get('/analysis/index.html', async (c) => {
        const path = storage.getAnalysisPath();
        try {
            const content = await readFile(path);
            return c.html(content.toString());
        } catch {
             // Try to download if missing
             try {
                const url = 'https://img.atcoder.jp/ahc_standings/index.html';
                const res = await fetch(url);
                if (res.ok) {
                    const html = await res.text();
                    await writeFile(path, html);
                    return c.html(html);
                }
             } catch {}
             return c.text('Analysis tool not found and failed to download automatically.', 404);
        }
    });

    app.get('/analysis/input.csv', async (c) => {
        const { parsed: rows } = getPahcerListParsed(baseDir);
        let csv = 'file,seed\n';
        for (const row of rows) {
            const file = row['Case'];
            if (file) {
                const seedMatch = file.match(/(\d+)/);
                const seed = seedMatch ? parseInt(seedMatch[1], 10) : 0;
                csv += `${file},${seed}\n`;
            }
        }
        return c.text(csv);
    });

    app.get('/analysis/result.csv', async (c) => {
        const { parsed: rows } = getPahcerListParsed(baseDir);
        let csv = 'author,file,score\n';
        const author = 'Current'; 
        for (const row of rows) {
            const file = row['Case'];
            const scoreStr = row['Score'];
            if (file && scoreStr) {
                const score = parseInt(scoreStr.replace(/,/g, ''), 10) || 0;
                csv += `${author},${file},${score}\n`;
            }
        }
        return c.text(csv);
    });

    // Static files
    const relativeDistDir = relative(process.cwd(), distDir);
    app.use('/*', serveStatic({ root: relativeDistDir }));
    app.get('*', serveStatic({ root: relativeDistDir, path: 'index.html' }));

    const port = parseInt(options.port);
    console.log(`Starting server on http://localhost:${port}`);
    
    try {
      serve({
        fetch: app.fetch,
        port
      }, (info) => {
        console.log(`Server is actually listening on http://localhost:${info.port}`);
      });
    } catch (err) {
      console.error('Failed to start server:', err);
    }
}
