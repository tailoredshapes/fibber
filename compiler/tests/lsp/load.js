'use strict';
// Load measurements of the language server: one big valid buffer (BYTES of defuns), the time of each request and the resident memory of the server,
// then CHANGES didChange of it; docs/shootout/lsp.md records what it measured.
// usage: node load.js SERVER BYTES [CHANGES]     SARGS="-I compiler" gives the server its arguments
const {spawn}=require('child_process');const fs=require('fs');
const bytes=+process.argv[3];const changes=+process.argv[4]||0;
const c=spawn('bash',['-c','ulimit -c 0; ulimit -v 8000000; exec "$0" "$@"',process.argv[2],...(process.env.SARGS||'').split(' ').filter(Boolean)],{stdio:['pipe','pipe','inherit']});
let buf=Buffer.alloc(0);const w=[];
c.stdout.on('data',d=>{buf=Buffer.concat([buf,d]);for(;;){const i=buf.indexOf('\r\n\r\n');if(i<0)return;const n=+/Content-Length: (\d+)/.exec(buf.slice(0,i).toString())[1];if(buf.length<i+4+n)return;const m=JSON.parse(buf.slice(i+4,i+4+n).toString());buf=buf.slice(i+4+n);const x=w.shift();if(x)x(m);}});
const send=m=>{const b=Buffer.from(JSON.stringify(m));c.stdin.write(Buffer.concat([Buffer.from(`Content-Length: ${b.length}\r\n\r\n`),b]))};
const next=(ms=300000)=>new Promise((r,j)=>{w.push(r);setTimeout(()=>j(new Error('timeout')),ms)});
const rss=()=>{try{return Math.round(+/VmRSS:\s+(\d+)/.exec(fs.readFileSync(`/proc/${c.pid}/status`,'utf8'))[1]/1024)}catch(e){return -1}};
// the bash wrapper execs: c.pid is the server (and after the re-exec it keeps its pid)
const uri='file:///tmp/big.fib';
let text='(ns x)\n';let k=0;while(text.length<bytes){text+=`(defun f${k} (a: i64) -> i64 (+ a ${k}))\n`;k++;}
text+='(defun main () -> i64 (f0 1))\n';
const lines=text.split('\n').length;
(async()=>{let t=Date.now();send({jsonrpc:'2.0',id:1,method:'initialize',params:{}});await next();
const lap=(l,x)=>{console.log(l.padEnd(26),String(Date.now()-t).padStart(7),'ms  rss',rss(),'MB',x||'');t=Date.now();};
lap('init');
send({jsonrpc:'2.0',method:'textDocument/didOpen',params:{textDocument:{uri,text}}});let d=await next();lap('didOpen '+text.length+' B',d.params.diagnostics.length+' diags '+(d.params.diagnostics[0]||{}).message);
send({jsonrpc:'2.0',method:'textDocument/didOpen',params:{textDocument:{uri,text}}});d=await next();lap('didOpen again');
const mid=Math.floor(lines/2);
for(const m of ['completion','hover','definition','documentSymbol']){send({jsonrpc:'2.0',id:2,method:'textDocument/'+m,params:{textDocument:{uri},position:{line:mid,character:12}}});const r=await next();lap(m,JSON.stringify(r).length+' B');}
for(let i=0;i<changes;i++){const t2=text+`(defun g${i} () -> i64 ${i})\n`;send({jsonrpc:'2.0',method:'textDocument/didChange',params:{textDocument:{uri},contentChanges:[{text:t2}]}});await next();
 if(i%Math.max(1,Math.floor(changes/5))===0||i===changes-1)lap(`didChange #${i}`);}
c.stdin.end();setTimeout(()=>process.exit(0),500);})().catch(e=>{console.log('FAILED',e.message);process.exit(1)});
