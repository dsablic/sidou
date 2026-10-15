// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Cracktro Chrome, SIDOU's logo: the source of its art (docs/art/README.md "Logo").
// Original art made for SIDOU, drawn in code pixel by pixel on a 240 x 240 grid in the
// 16 C64 colours (Pepto's palette, docs/art/README.md "Palette"), and nothing else.
//
// The drawing code below is the logo page's (variant V2, "Cracktro Chrome"), copied verbatim:
// v2 the 240 x 240 scene (v2(true): without the background), a2 the 48 x 48 avatar, chrome
// the bevelled lettering with its extruded shadow. Each draws through R and D, which fill
// whole pixels of a 2D canvas context X with a palette colour.
//
//   node assets/logo/cracktro-chrome.js
//
// renders them with whole-pixel fills (no canvas needed: every fill is on whole pixels, and
// a fill that is not throws) and writes the indexed PNGs:
// - assets/logo/cracktro-chrome.png: the scene, 240 x 240, 4 bits a pixel in the art
//   palette's order (index i is GRID_ colour i, app/grid.h). The firmware's bitmap is
//   generated from it (host/crates/sidou-tests/src/logo.rs, app/logo_bitmap.c).
// - assets/logo/cracktro-chrome-lettering.png and -lettering-2x.png: the lettering and its
//   shadow alone, cropped, on transparency (palette index 16), for the README; the 2x is
//   scaled by nearest neighbour.
// - docs/art/avatar.png: the avatar, 48 x 48.
// Its pixels were checked against Chromium's canvas drawing the logo page (all equal).

const C={k:'#000000',w:'#ffffff',r:'#68372b',c:'#70a4b2',p:'#6f3d86',g:'#588d43',b:'#352879',y:'#b8c76f',o:'#6f4f25',n:'#433900',R:'#9a6759',d:'#444444',m:'#6c6c6c',G:'#9ad284',B:'#6c5eb5',l:'#959595'};
const R=(x,y,w,h,c)=>{X.fillStyle=C[c];X.fillRect(x,y,w,h)};
const D=(x,y,c)=>{X.fillStyle=C[c];X.fillRect(x,y,1,1)};
function line(x0,y0,x1,y1,c,t=1){const dx=Math.abs(x1-x0),dy=-Math.abs(y1-y0),sx=x0<x1?1:-1,sy=y0<y1?1:-1;let e=dx+dy;for(;;){R(x0,y0,t,t,c);if(x0===x1&&y0===y1)break;const e2=2*e;if(e2>=dy){e+=dy;x0+=sx}if(e2<=dx){e+=dx;y0+=sy}}}
function rng(seed){let s=seed;return()=>(s=(s*1103515245+12345)&0x7fffffff)/0x7fffffff}
const F3={A:['.#.','#.#','###','#.#','#.#'],B:['##.','#.#','##.','#.#','##.'],C:['.##','#..','#..','#..','.##'],D:['##.','#.#','#.#','#.#','##.'],E:['###','#..','##.','#..','###'],F:['###','#..','##.','#..','#..'],G:['.##','#..','#.#','#.#','.##'],H:['#.#','#.#','###','#.#','#.#'],I:['###','.#.','.#.','.#.','###'],K:['#.#','#.#','##.','#.#','#.#'],L:['#..','#..','#..','#..','###'],M:['#.#','###','###','#.#','#.#'],N:['##.','#.#','#.#','#.#','#.#'],O:['.#.','#.#','#.#','#.#','.#.'],P:['##.','#.#','##.','#..','#..'],R:['##.','#.#','##.','#.#','#.#'],S:['.##','#..','.#.','..#','##.'],T:['###','.#.','.#.','.#.','.#.'],U:['#.#','#.#','#.#','#.#','###'],V:['#.#','#.#','#.#','#.#','.#.'],W:['#.#','#.#','###','###','#.#'],X:['#.#','#.#','.#.','#.#','#.#'],Y:['#.#','#.#','.#.','.#.','.#.'],Z:['###','..#','.#.','#..','###'],
'0':['###','#.#','#.#','#.#','###'],'1':['.#.','##.','.#.','.#.','###'],'2':['##.','..#','.#.','#..','###'],'3':['##.','..#','.#.','..#','##.'],'4':['#.#','#.#','###','..#','..#'],'5':['###','#..','##.','..#','##.'],'6':['.##','#..','###','#.#','###'],'7':['###','..#','.#.','.#.','.#.'],'8':['###','#.#','###','#.#','###'],'9':['###','#.#','###','..#','##.'],
'.':['...','...','...','...','.#.'],'-':['...','...','###','...','...'],'·':['...','...','.#.','...','...'],':':['...','.#.','...','.#.','...'],'/':['..#','..#','.#.','#..','#..'],'*':['...','#.#','.#.','#.#','...'],' ':['...','...','...','...','...']};
function txt(s,x,y,c,sc=1){[...s].forEach((ch,i)=>{const g=F3[ch]||F3[' '];g.forEach((row,j)=>{[...row].forEach((p,k)=>{if(p==='#')R(x+(i*4+k)*sc,y+j*sc,sc,sc,c)})})})}
const tw=(s,sc=1)=>s.length*4*sc-sc;
const ctxt=(s,cx,y,c,sc=1)=>txt(s,Math.round(cx-tw(s,sc)/2),y,c,sc);
const G5={S:['.####','##...','##...','.###.','...##','...##','####.'],I:['####','.##.','.##.','.##.','.##.','.##.','####'],D:['####.','##.##','##.##','##.##','##.##','##.##','####.'],O:['.###.','##.##','##.##','##.##','##.##','##.##','.###.'],U:['##.##','##.##','##.##','##.##','##.##','##.##','.###.']};
function mask(w,h){const m=new Uint8Array(w*h);return{w,h,m,at(x,y){return x<0||y<0||x>=w||y>=h?0:m[y*w+x]},set(x,y,v=1){if(x>=0&&y>=0&&x<w&&y<h)m[y*w+x]=v}}}
function wmask(text,cell){const gl=[...text].map(ch=>G5[ch]);const cw=gl.reduce((a,g)=>a+g[0].length,0)+gl.length-1;const M=mask(cw*cell,7*cell);let cx=0;gl.forEach(g=>{g.forEach((row,j)=>[...row].forEach((p,i)=>{if(p==='#')for(let y=0;y<cell;y++)for(let x=0;x<cell;x++)M.set((cx+i)*cell+x,j*cell+y)}));cx+=g[0].length+1});return M}
function chamfer(M){const o=mask(M.w,M.h);o.m.set(M.m);for(let y=0;y<M.h;y++)for(let x=0;x<M.w;x++){if(!o.at(x,y))continue;const u=!o.at(x,y-1),d=!o.at(x,y+1),l=!o.at(x-1,y),r=!o.at(x+1,y);if((u&&l)||(u&&r)||(d&&l)||(d&&r))M.set(x,y,0)}return M}
function paint(M,ox,oy,fn){for(let y=0;y<M.h;y++)for(let x=0;x<M.w;x++){if(!M.at(x,y))continue;const c=typeof fn==='string'?fn:fn(x,y);if(c)D(ox+x,oy+y,c)}}
function outline(M,ox,oy,c){for(let y=-1;y<=M.h;y++)for(let x=-1;x<=M.w;x++){if(M.at(x,y))continue;let n=0;for(let j=-1;j<=1;j++)for(let i=-1;i<=1;i++)n|=M.at(x+i,y+j);if(n)D(ox+x,oy+y,c)}}
function chrome(text,ox,oy,cell,ext){const M=chamfer(wmask(text,cell));const h=M.h;const bands=['w','w','l','c','c','B','b','n','o','R','R','y','y','w'];
  for(let k=ext;k>=1;k--)paint(M,ox+k,oy+k,'p');outline(M,ox,oy,'k');
  paint(M,ox,oy,(x,y)=>{if(!M.at(x,y-1))return'w';if(!M.at(x-1,y))return y<h/2?'w':'y';if(!M.at(x+1,y)||!M.at(x,y+1))return y<h/2?'b':'n';return bands[Math.min(13,Math.floor(y*14/h))]})}
function bar(y,seq){seq.forEach((c,i)=>R(0,y+i*2,240,2,c))}
function v2(tr){
  if(!tr){R(0,0,240,240,'k');const r=rng(64);for(let i=0;i<70;i++){const x=Math.floor(r()*240),y=44+Math.floor(r()*118);D(x,y,['m','l','w','B','d'][i%5])}
    [[30,58],[214,70],[176,150],[22,140]].forEach(([x,y])=>{D(x,y,'w');D(x-1,y,'l');D(x+1,y,'l');D(x,y-1,'l');D(x,y+1,'l');D(x-2,y,'d');D(x+2,y,'d')});
    for(let i=0;i<8;i++){const y=172+Math.round(i*i*0.55+i*1.2);R(0,y,240,1,i<2?'b':'p')}
    for(let i=-12;i<=12;i++){line(120+i*10,172,120+i*34,205,i%2?'b':'p')}
    R(0,170,240,2,'B');
    bar(22,['b','B','c','l','w','l','c','B','b']);bar(206,['n','o','R','y','w','y','R','o','n'])}
  ctxt('A NEW VOICE FOR THE FM-1',120,52,'l');
  chrome('SIDOU',6,76,8,4);
  ctxt('6581 · 8580 · TRACKER · SYNTH',120,150,'c');
}
function a2(){R(0,0,48,48,'k');const r=rng(9);for(let i=0;i<14;i++)D(Math.floor(r()*48),Math.floor(r()*48),i%2?'l':'m');chrome('S',10,5,5,3);R(0,43,48,1,'B');R(0,45,48,1,'b')}

// ---- not from the page: the lettering alone, and the export ----

// The scene's lettering and its shadow, where the scene has them.
function lettering(){chrome('SIDOU',6,76,8,4)}

let X;

// A context of whole-pixel fills into palette indexes (255: nothing drawn).
function pixelContext(w,h){const index=new Map(Object.keys(C).map((k,i)=>[C[k],i]));const px=new Uint8Array(w*h).fill(255);let fill=0;
  return{w,h,px,set fillStyle(c){fill=index.get(c)},fillRect(x,y,rw,rh){if(![x,y,rw,rh].every(Number.isInteger))throw new Error(`fill (${x}, ${y}, ${rw}, ${rh}) is not on whole pixels`);
    for(let j=Math.max(0,y);j<Math.min(h,y+rh);j++)for(let i=Math.max(0,x);i<Math.min(w,x+rw);i++)px[j*w+i]=fill}}}

function draw(fn,w,h){X=pixelContext(w,h);fn();const out=X;X=undefined;return out}

// The smallest rectangle holding every drawn pixel.
function crop(img){let x0=img.w,y0=img.h,x1=-1,y1=-1;for(let y=0;y<img.h;y++)for(let x=0;x<img.w;x++)if(img.px[y*img.w+x]!==255){x0=Math.min(x0,x);x1=Math.max(x1,x);y0=Math.min(y0,y);y1=Math.max(y1,y)}
  const w=x1-x0+1,h=y1-y0+1,px=new Uint8Array(w*h);for(let y=0;y<h;y++)for(let x=0;x<w;x++)px[y*w+x]=img.px[(y0+y)*img.w+x0+x];return{w,h,px}}

function scale(img,k){const w=img.w*k,h=img.h*k,px=new Uint8Array(w*h);for(let y=0;y<h;y++)for(let x=0;x<w;x++)px[y*w+x]=img.px[Math.floor(y/k)*img.w+Math.floor(x/k)];return{w,h,px}}

// An indexed PNG: 4 bits a pixel and the 16 colours for an opaque image, else 8 bits and a
// 17th, transparent entry for the pixels nothing drew.
function png(img){const zlib=require('zlib');const clear=img.px.includes(255);const depth=clear?8:4;
  const crcTable=Array.from({length:256},(_,n)=>{let c=n;for(let k=0;k<8;k++)c=c&1?0xedb88320^(c>>>1):c>>>1;return c>>>0});
  const crc=b=>{let c=0xffffffff;for(const v of b)c=crcTable[(c^v)&255]^(c>>>8);return(c^0xffffffff)>>>0};
  const chunk=(type,data)=>{const t=Buffer.from(type,'ascii'),len=Buffer.alloc(4),sum=Buffer.alloc(4);len.writeUInt32BE(data.length);sum.writeUInt32BE(crc(Buffer.concat([t,data])));return Buffer.concat([len,t,data,sum])};
  const ihdr=Buffer.alloc(13);ihdr.writeUInt32BE(img.w,0);ihdr.writeUInt32BE(img.h,4);ihdr[8]=depth;ihdr[9]=3;
  const rgb=Object.values(C).map(h=>[1,3,5].map(i=>parseInt(h.slice(i,i+2),16)));if(clear)rgb.push([0,0,0]);
  const plte=Buffer.from(rgb.flat());const stride=Math.ceil(img.w*depth/8);const raw=Buffer.alloc((stride+1)*img.h);
  for(let y=0;y<img.h;y++)for(let x=0;x<img.w;x++){const v=img.px[y*img.w+x]===255?16:img.px[y*img.w+x],o=y*(stride+1)+1;if(depth===8)raw[o+x]=v;else raw[o+(x>>1)]|=x&1?v:v<<4}
  const parts=[Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',ihdr),chunk('PLTE',plte)];
  if(clear){const t=Buffer.alloc(17,255);t[16]=0;parts.push(chunk('tRNS',t))}
  parts.push(chunk('IDAT',zlib.deflateSync(raw,{level:9})),chunk('IEND',Buffer.alloc(0)));return Buffer.concat(parts)}

if(typeof module!=='undefined'&&require.main===module){const fs=require('fs'),path=require('path');const here=__dirname,root=path.join(here,'../..');
  const word=crop(draw(lettering,240,240));
  const out={[path.join(here,'cracktro-chrome.png')]:draw(()=>v2(false),240,240),[path.join(here,'cracktro-chrome-lettering.png')]:word,
    [path.join(here,'cracktro-chrome-lettering-2x.png')]:scale(word,2),[path.join(root,'docs/art/avatar.png')]:draw(a2,48,48)};
  for(const[file,img]of Object.entries(out)){fs.writeFileSync(file,png(img));console.log(`${path.relative(root,file)}: ${img.w} x ${img.h}`)}}
