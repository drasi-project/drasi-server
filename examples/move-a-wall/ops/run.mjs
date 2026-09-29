import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir, open } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
process.chdir(root);
for (const port of [8421,8422,8423,5421]) {
  await new Promise((accept,reject) => {
    const socket = createServer();
    socket.once('error',reject);
    socket.listen(port,'127.0.0.1',() => socket.close(accept));
  });
}
await mkdir('.build',{recursive:true});
const log = await open('.build/server.log','a');
const children = [];
let stopping = false;
let failed = false;
async function stop(code) {
  if (stopping) return;
  stopping = true;
  const outcomes = children.map(child => new Promise(accept => {
    if (child.exitCode !== null || child.signalCode !== null) return accept();
    child.once('exit',accept);
    child.kill('SIGINT');
  }));
  const timer = setTimeout(() => {
    console.error('Timed out waiting for owned processes to stop.');
    failed = true;
    for (const child of children) if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
  },15000);
  await Promise.all(outcomes);
  clearTimeout(timer);
  await log.close();
  process.exit(failed ? 1 : code);
}
function run(binary,args,options) {
  const child = spawn(binary,args,{cwd:root,...options});
  children.push(child);
  child.once('error',error => { console.error(error); failed = true; void stop(1); });
  child.once('exit',(code,signal) => {
    if (!stopping) { console.error(`${binary} exited (${code ?? signal}). See .build/server.log.`); failed = true; void stop(1); }
    else if (code !== 0 && signal !== 'SIGINT') { failed = true; console.error(`${binary} shutdown: ${code ?? signal}`); }
  });
  return child;
}
for (const signal of ['SIGINT','SIGTERM']) process.on(signal,() => void stop(0));
run(resolve('target/debug/drasi-server'),['--config',resolve('.build/server.yaml'),'--plugins-dir',resolve('.build/plugins'),'--skip-verification','--disable-ui'],
  {cwd:resolve('.build'),stdio:['ignore',log.fd,log.fd],env:{...process.env,RUST_LOG:process.env.RUST_LOG ?? 'info'}});
try {
  let ready = false;
  for (let n=0;n<120 && !stopping;n++) {
    try {
      const response = await fetch('http://127.0.0.1:8421/api/v1/instances/move-a-wall/queries/geometry-status/results',{signal:AbortSignal.timeout(1500)});
      if (response.ok) {
        const body = await response.json();
        if (body.success && body.data?.some(row => row.revision === 1)) { ready = true; break; }
      }
    } catch (error) {
      if (n === 119) console.error('Readiness request failed:',error.message);
    }
    await new Promise(resolve => setTimeout(resolve,500));
  }
  if (!ready) throw new Error('Stock drasi-server did not produce the initial geometry-status query result. See .build/server.log.');
  run(process.execPath,['ui/node_modules/vite/bin/vite.js','--config','ui/vite.config.ts','--host','127.0.0.1','--port','5421','--strictPort','ui'],{stdio:'inherit'});
  console.log('\nMove a Wall: http://127.0.0.1:5421\nStock drasi-server API 8421 · SSE 8422 · scene commands 8423\nCtrl-C stops only this launcher’s processes.');
} catch (error) { console.error(error); await stop(1); }
