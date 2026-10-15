// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Metal Chrome, SIDOU's logo: the source of its art (docs/art/README.md "Logo").
// Original art made for SIDOU, drawn in code pixel by pixel on a 240 x 240 grid in the
// 16 C64 colours (Pepto's palette, docs/art/README.md "Palette"), and nothing else.
//
// The drawing code below is the logo page's (variant M1, "Metal Chrome"), copied verbatim:
// v9 the 240 x 240 scene (v9(true): without the background), a9 the 48 x 48 avatar, vm the
// scene (lettering, glow, bars, stars, glint and tagline), mmask the metal alphabet (MG),
// metal its chrome bevel, halo and airbrush its dithered glow. Each draws through R and D,
// which fill whole pixels of a 2D canvas context X with a palette colour.
//
//   node assets/logo/metal-chrome.js
//
// renders them with whole-pixel fills (no canvas needed: every fill is on whole pixels, and
// a fill that is not throws) and writes the indexed PNGs:
// - assets/logo/metal-chrome.png: the scene, 240 x 240, 4 bits a pixel in the art palette's
//   order (index i is GRID_ colour i, app/grid.h). The firmware's bitmap is generated from
//   it (host/crates/sidou-tests/src/logo.rs, app/logo_bitmap.c).
// - assets/logo/metal-chrome-lettering.png and -lettering-2x.png: the lettering, its
//   outline and its extruded shadow alone, cropped, on transparency (palette index 16), for
//   the README; the 2x is scaled by nearest neighbour.
// - docs/art/avatar.png: the avatar, 48 x 48 (Cracktro Chrome's is
//   assets/logo/cracktro-chrome-avatar.png).
// Its pixels were checked against Chromium's canvas drawing the logo page (all equal).

const C={k:'#000000',w:'#ffffff',r:'#68372b',c:'#70a4b2',p:'#6f3d86',g:'#588d43',b:'#352879',y:'#b8c76f',o:'#6f4f25',n:'#433900',R:'#9a6759',d:'#444444',m:'#6c6c6c',G:'#9ad284',B:'#6c5eb5',l:'#959595'};
const R=(x,y,w,h,c)=>{X.fillStyle=C[c];X.fillRect(x,y,w,h)};
const D=(x,y,c)=>{X.fillStyle=C[c];X.fillRect(x,y,1,1)};
function rng(seed){let s=seed;return()=>(s=(s*1103515245+12345)&0x7fffffff)/0x7fffffff}
const F3={A:['.#.','#.#','###','#.#','#.#'],B:['##.','#.#','##.','#.#','##.'],C:['.##','#..','#..','#..','.##'],D:['##.','#.#','#.#','#.#','##.'],E:['###','#..','##.','#..','###'],F:['###','#..','##.','#..','#..'],G:['.##','#..','#.#','#.#','.##'],H:['#.#','#.#','###','#.#','#.#'],I:['###','.#.','.#.','.#.','###'],K:['#.#','#.#','##.','#.#','#.#'],L:['#..','#..','#..','#..','###'],M:['#.#','###','###','#.#','#.#'],N:['##.','#.#','#.#','#.#','#.#'],O:['.#.','#.#','#.#','#.#','.#.'],P:['##.','#.#','##.','#..','#..'],R:['##.','#.#','##.','#.#','#.#'],S:['.##','#..','.#.','..#','##.'],T:['###','.#.','.#.','.#.','.#.'],U:['#.#','#.#','#.#','#.#','###'],V:['#.#','#.#','#.#','#.#','.#.'],W:['#.#','#.#','###','###','#.#'],X:['#.#','#.#','.#.','#.#','#.#'],Y:['#.#','#.#','.#.','.#.','.#.'],Z:['###','..#','.#.','#..','###'],
'0':['###','#.#','#.#','#.#','###'],'1':['.#.','##.','.#.','.#.','###'],'2':['##.','..#','.#.','#..','###'],'3':['##.','..#','.#.','..#','##.'],'4':['#.#','#.#','###','..#','..#'],'5':['###','#..','##.','..#','##.'],'6':['.##','#..','###','#.#','###'],'7':['###','..#','.#.','.#.','.#.'],'8':['###','#.#','###','#.#','###'],'9':['###','#.#','###','..#','##.'],
'.':['...','...','...','...','.#.'],'-':['...','...','###','...','...'],'·':['...','...','.#.','...','...'],':':['...','.#.','...','.#.','...'],'/':['..#','..#','.#.','#..','#..'],'*':['...','#.#','.#.','#.#','...'],' ':['...','...','...','...','...']};
function txt(s,x,y,c,sc=1){[...s].forEach((ch,i)=>{const g=F3[ch]||F3[' '];g.forEach((row,j)=>{[...row].forEach((p,k)=>{if(p==='#')R(x+(i*4+k)*sc,y+j*sc,sc,sc,c)})})})}
const tw=(s,sc=1)=>s.length*4*sc-sc;
const ctxt=(s,cx,y,c,sc=1)=>txt(s,Math.round(cx-tw(s,sc)/2),y,c,sc);
function mask(w,h){const m=new Uint8Array(w*h);return{w,h,m,at(x,y){return x<0||y<0||x>=w||y>=h?0:m[y*w+x]},set(x,y,v=1){if(x>=0&&y>=0&&x<w&&y<h)m[y*w+x]=v}}}
function paint(M,ox,oy,fn){for(let y=0;y<M.h;y++)for(let x=0;x<M.w;x++){if(!M.at(x,y))continue;const c=typeof fn==='string'?fn:fn(x,y);if(c)D(ox+x,oy+y,c)}}
function outline(M,ox,oy,c){for(let y=-1;y<=M.h;y++)for(let x=-1;x<=M.w;x++){if(M.at(x,y))continue;let n=0;for(let j=-1;j<=1;j++)for(let i=-1;i<=1;i++)n|=M.at(x+i,y+j);if(n)D(ox+x,oy+y,c)}}
function bar(y,seq){seq.forEach((c,i)=>R(0,y+i*2,240,2,c))}
/* ---------- Metal glyphs: 6 cells wide, 11 rows (spike row, 9 body rows, spike row) ---------- */
/* # full · a ◢ · b ◣ · c ◤ · d ◥ · ^ spike up · v spike down. Spike rows are drawn taller than body rows. */
const MG={
 S:['.....a','.a####','a#####','##b..d','d##b..','.d##b.','..d##b','b..d##','#####c','####c.','c.....'],
 I:['b....a','######','.d##c.','..##..','..##..','..##..','..##..','..##..','.a##b.','######','c....d'],
 D:['b.....','####b.','#####b','##cd##','##..##','##..##','##..##','##ba##','#####c','####c.','c.....'],
 O:['..ab..','.a##b.','a####b','##cd##','##..##','##..##','##..##','##ba##','d####c','.d##c.','..dc..'],
 U:['ab..ab','##..##','##..##','##..##','##..##','##..##','##..##','##ba##','d####c','.d##c.','..dc..'],
 T:['b....a','######','c.##.d','..##..','..##..','..##..','..##..','..##..','..##..','..##..','..dc..'],
 H:['ab..ab','##..##','##..##','##..##','######','##dc##','##..##','##..##','##..##','##..##','dc..dc'],
 E:['b....a','######','##...d','##....','####b.','####c.','##....','##....','##...a','######','c....d'],
 F:['b....a','######','##...d','##....','####b.','####c.','##....','##....','##....','##....','dc....'],
 L:['ab....','##....','##....','##....','##....','##....','##....','##....','##...a','######','c....d'],
 C:['.....a','.a####','a#####','##...d','##....','##....','##....','##...a','d#####','.d####','.....d'],
 A:['..ab..','.a##b.','a####b','##cd##','##..##','######','##..##','##..##','##..##','##..##','dc..dc'],
 R:['b.....','####b.','#####b','##cd##','##ba##','#####c','##d#b.','##.d#b','##..##','##..##','dc..dc'],
 N:['ab..ab','##b.##','###b##','##d#b#','##.d##','##..##','##..##','##..##','##..##','##..##','dc..dc']
};
function cellIn(c,u,v){switch(c){case'#':return true;case'a':return u+v>=1;case'b':return v>=u;case'c':return u+v<=1;case'd':return v<=u;case'^':return Math.abs(u-.5)<=v/2;case'v':return Math.abs(u-.5)<=(1-v)/2}return false}
function mmask(text,cw,ch,sh,gap){const gl=[...text].map(c=>MG[c]||null);const gw=g=>(g?g[0].length:3)*cw;
  const W=gl.reduce((a,g)=>a+gw(g),0)+gap*(gl.length-1),rows=[sh,...Array(9).fill(ch),sh],M=mask(W,2*sh+9*ch);let gx=0;
  gl.forEach(g=>{if(g){let y0=0;g.forEach((row,j)=>{const rh=rows[j];[...row].forEach((c,i)=>{if(c==='.')return;for(let py=0;py<rh;py++)for(let px=0;px<cw;px++)if(cellIn(c,(px+.5)/cw,(py+.5)/rh))M.set(gx+i*cw+px,y0+py)});y0+=rh})}gx+=gw(g)+gap});
  M.top=sh;M.bot=sh+9*ch;return M}

const BAY=[0,8,2,10,12,4,14,6,3,11,1,9,15,7,13,5];
const bay=(x,y)=>(BAY[(y&3)*4+(x&3)]+.5)/16;
function dither(x,y,t,stops){if(t<=0)return null;const L=Math.min(1,t)*(stops.length-1),i=Math.floor(L),c=(L-i)>bay(x,y)?stops[Math.min(i+1,stops.length-1)]:stops[i];return c==='k'&&stops[0]==='k'?null:c}
function dfield(M,pad){const w=M.w+2*pad,h=M.h+2*pad,d=new Float32Array(w*h).fill(1e9),S=Math.SQRT2;
  for(let y=0;y<h;y++)for(let x=0;x<w;x++)if(M.at(x-pad,y-pad))d[y*w+x]=0;
  for(let y=0;y<h;y++)for(let x=0;x<w;x++){const i=y*w+x;let v=d[i];if(x>0)v=Math.min(v,d[i-1]+1);if(y>0){v=Math.min(v,d[i-w]+1);if(x>0)v=Math.min(v,d[i-w-1]+S);if(x<w-1)v=Math.min(v,d[i-w+1]+S)}d[i]=v}
  for(let y=h-1;y>=0;y--)for(let x=w-1;x>=0;x--){const i=y*w+x;let v=d[i];if(x<w-1)v=Math.min(v,d[i+1]+1);if(y<h-1){v=Math.min(v,d[i+w]+1);if(x<w-1)v=Math.min(v,d[i+w+1]+S);if(x>0)v=Math.min(v,d[i+w-1]+S)}d[i]=v}
  return{w,h,pad,d}}
function halo(M,ox,oy,r,stops,amt=1,dy=0){const F=dfield(M,r+1);for(let y=0;y<F.h;y++)for(let x=0;x<F.w;x++){const v=F.d[y*F.w+x];if(v<1.5||v>r)continue;const X0=ox+x-F.pad,Y0=oy+y-F.pad+dy,c=dither(X0,Y0,amt*Math.pow(1-v/r,1.35),stops);if(c)D(X0,Y0,c)}}
function airbrush(cx,cy,rx,ry,stops,amt=1,y0=0,y1=240){for(let y=y0;y<y1;y++)for(let x=0;x<240;x++){const e=Math.hypot((x+.5-cx)/rx,(y+.5-cy)/ry);if(e>=1)continue;const c=dither(x,y,amt*(1-e)*(1-e)*1.6,stops);if(c)D(x,y,c)}}

/* chrome bevel: top lip, lit up-left faces, shaded down-right faces, banded core */
function metal(M,ox,oy,s){const t=M.top,b=M.bot,n=s.ramp.length;
  for(let k=s.ext||0;k>=1;k--)paint(M,ox+k,oy+k,s.shc);
  outline(M,ox,oy,'k');
  paint(M,ox,oy,(x,y)=>{const up=!M.at(x,y-1),lf=!M.at(x-1,y),ul=!M.at(x-1,y-1),dn=!M.at(x,y+1),rt=!M.at(x+1,y),dr=!M.at(x+1,y+1);
    if(up&&!dn)return s.hi;if(lf||ul)return s.lit;if(dn||rt||dr)return s.shd;
    if(s.inner&&(!M.at(x,y-2)||!M.at(x-2,y)))return s.inner;
    const v=(y-t)/(b-t);return s.ramp[Math.max(0,Math.min(n-1,Math.floor(v*n)))]})}
const HOT={ramp:['r','r','R','R','o','y','y','w'],hi:'w',lit:'y',shd:'r',inner:'R',ext:3,shc:'p'};
const ICE={ramp:['b','p','p','B','B','l','w','w'],hi:'w',lit:'l',shd:'b',inner:'B',ext:3,shc:'r'};

function star(x,y,big){D(x,y,'w');D(x-1,y,'l');D(x+1,y,'l');D(x,y-1,'l');D(x,y+1,'l');if(big){D(x-2,y,'m');D(x+2,y,'m');D(x,y-2,'m');D(x,y+2,'m')}}
function flare(x,y,l){for(let i=-l;i<=l;i++){const a=Math.abs(i),c=a<2?'w':a<l*.4?'l':a<l*.75?'m':'d';D(x+i,y,c);if(a<=l*.55)D(x,y+i,a<2?'w':a<l*.25?'l':a<l*.45?'m':'d')}
  for(let i=1;i<=3;i++){const c=i<2?'l':'d';D(x-i,y-i,c);D(x+i,y-i,c);D(x-i,y+i,c);D(x+i,y+i,c)}R(x-1,y-1,3,3,'w')}

/* ---------- M1 Metal Chrome ---------- */
function mstars(seed,n,y0,y1){const r=rng(seed);for(let i=0;i<n;i++){const x=Math.floor(r()*240),y=y0+Math.floor(r()*(y1-y0));D(x,y,['d','m','B','l','p'][i%5])}}
function hotbars(cold){const q=cold?['k','b','k','b','p','b','B','p','B','w']:['k','r','k','p','r','p','R','p','R','w'];bar(18,q);bar(202,[...q].reverse())}
function vm(tr,s,glowStops,under,tag){const M=mmask('SIDOU',7,5,8,3),ox=Math.floor((240-M.w)/2),oy=82;
  if(!tr){R(0,0,240,240,'k');mstars(s===HOT?66:67,46,40,196);
    airbrush(120,oy+32,150,56,glowStops,.55);
    halo(M,ox,oy,13,glowStops,1);if(under)halo(M,ox,oy,12,under,1,7);
    hotbars(s===ICE);[[22,52],[218,60],[30,178],[206,170]].forEach(([x,y],i)=>star(x,y,i%2))}
  metal(M,ox,oy,s);
  if(!tr)flare(ox+M.w-4,oy+M.top+1,15);
  ctxt(tag,120,oy+M.h+20,s===HOT?'R':'B');
  ctxt('6581 · 8580 · TRACKER · SYNTH',120,oy+M.h+32,'m');
}
function v9(tr){vm(tr,HOT,['k','b','p','R'],null,'FORGED IN SILICON FOR THE FM-1')}
function a9(){R(0,0,48,48,'k');const M=mmask('S',4,3,4,0);halo(M,14,5,7,['k','b','p','R']);metal(M,14,5,{...HOT,ext:2,inner:null});star(6,8,0);star(41,39,0)}

// ---- not from the page: the lettering alone, and the export ----

// The scene's lettering, its outline and its shadow, where the scene has them (vm's).
function lettering(){const M=mmask('SIDOU',7,5,8,3);metal(M,Math.floor((240-M.w)/2),82,HOT)}

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
  const out={[path.join(here,'metal-chrome.png')]:draw(()=>v9(false),240,240),[path.join(here,'metal-chrome-lettering.png')]:word,
    [path.join(here,'metal-chrome-lettering-2x.png')]:scale(word,2),[path.join(root,'docs/art/avatar.png')]:draw(a9,48,48)};
  for(const[file,img]of Object.entries(out)){fs.writeFileSync(file,png(img));console.log(`${path.relative(root,file)}: ${img.w} x ${img.h}`)}}
