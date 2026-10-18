// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The report: one self-contained HTML file. Screenshots are shown at calibrated real size
//! (the calibration idea of assets/prototypes/screen-readability.html: a bank card against a
//! bar, then the screen's width in mm) and at 1:1 pixels; knob findings carry a chart of value
//! against detent; audio clips are embedded as data URIs.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::findings::{Chart, Finding, Sev};

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A chart as inline SVG: series each scaled 0..100 by the caller.
pub fn svg_chart(c: &Chart) -> String {
    let (w, h, l, b, t) = (380.0, 170.0, 30.0, 26.0, 8.0);
    let xmax = c
        .series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.0))
        .fold(1.0, f64::max);
    let colours = ["var(--c1)", "var(--c2)", "var(--c3)"];
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" class=\"chart\" role=\"img\" aria-label=\"{}\">",
        esc(&c.title)
    );
    for g in 0..=4 {
        let y = t + (h - b - t) * f64::from(g) / 4.0;
        s += &format!("<line x1=\"{l}\" y1=\"{y:.1}\" x2=\"{w}\" y2=\"{y:.1}\" class=\"grid\"/>");
        s += &format!(
            "<text x=\"{:.0}\" y=\"{:.1}\" class=\"tick\">{}</text>",
            2,
            y + 3.0,
            100 - g * 25
        );
    }
    for (i, ser) in c.series.iter().enumerate() {
        let pts: Vec<String> = ser
            .points
            .iter()
            .map(|(x, y)| {
                format!(
                    "{:.1},{:.1}",
                    l + (w - l - 4.0) * x / xmax,
                    t + (h - b - t) * (1.0 - y / 100.0)
                )
            })
            .collect();
        s += &format!(
            "<polyline fill=\"none\" stroke=\"{}\" stroke-width=\"1.8\" points=\"{}\"/>",
            colours[i % 3],
            pts.join(" ")
        );
    }
    s += &format!(
        "<text x=\"{:.0}\" y=\"{:.0}\" class=\"tick\" text-anchor=\"middle\">{} (0 to {:.0})</text>",
        (l + w) / 2.0,
        h - 8.0,
        esc(&c.x_label),
        xmax
    );
    s += "</svg>";
    let legend: Vec<String> = c
        .series
        .iter()
        .enumerate()
        .map(|(i, ser)| {
            format!(
                "<span><i style=\"background:{}\"></i>{}</span>",
                colours[i % 3],
                esc(&ser.name)
            )
        })
        .collect();
    format!(
        "<figure class=\"chartfig\">{s}<figcaption>{}<br>{}</figcaption></figure>",
        esc(&c.y_label),
        legend.join("")
    )
}

pub struct Assets<'a> {
    pub out: &'a Path,
    pub imgs: BTreeMap<String, String>,
    pub auds: BTreeMap<String, String>,
}

impl<'a> Assets<'a> {
    pub fn new(out: &'a Path) -> Self {
        Assets {
            out,
            imgs: BTreeMap::new(),
            auds: BTreeMap::new(),
        }
    }

    fn img(&mut self, rel: &str) -> bool {
        if self.imgs.contains_key(rel) {
            return true;
        }
        match std::fs::read(self.out.join(rel)) {
            Ok(b) => {
                self.imgs.insert(
                    rel.to_string(),
                    format!("data:image/png;base64,{}", base64(&b)),
                );
                true
            }
            Err(_) => false,
        }
    }

    fn aud(&mut self, rel: &str) -> bool {
        if self.auds.contains_key(rel) {
            return true;
        }
        match std::fs::read(self.out.join(rel)) {
            Ok(b) => {
                self.auds.insert(
                    rel.to_string(),
                    format!("data:audio/wav;base64,{}", base64(&b)),
                );
                true
            }
            Err(_) => false,
        }
    }

    /// A screenshot at calibrated real size and at 1:1, with overlay boxes.
    pub fn shot(
        &mut self,
        rel: &str,
        caption: &str,
        overlays: &[(usize, usize, usize, usize, String)],
    ) -> String {
        if !self.img(rel) {
            return String::new();
        }
        let boxes = |o: &[(usize, usize, usize, usize, String)]| -> String {
            o.iter()
                .map(|(c, r, w, h, l)| {
                    format!(
                        "<b class=\"ov\" style=\"left:{:.2}%;top:{:.2}%;width:{:.2}%;height:{:.2}%\" title=\"{}\"></b>",
                        *c as f64 / 30.0 * 100.0,
                        *r as f64 / 30.0 * 100.0,
                        *w as f64 / 30.0 * 100.0,
                        *h as f64 / 30.0 * 100.0,
                        esc(l)
                    )
                })
                .collect()
        };
        let b = boxes(overlays);
        format!(
            "<figure class=\"shot\"><div class=\"pair\">\
             <div><div class=\"bezel real\"><img data-img=\"{k}\" alt=\"{a}\">{b}</div><div class=\"scale\">real size</div></div>\
             <div><div class=\"bezel one\"><img data-img=\"{k}\" alt=\"{a}\" width=\"240\" height=\"240\">{b}</div><div class=\"scale\">1:1 pixels</div></div>\
             </div><figcaption>{a}</figcaption></figure>",
            k = esc(rel),
            a = esc(caption),
            b = b
        )
    }

    pub fn audio(&mut self, rel: &str, label: &str) -> String {
        if !self.aud(rel) {
            return String::new();
        }
        format!(
            "<div class=\"clip\"><span>{}</span><audio controls preload=\"none\" data-aud=\"{}\"></audio></div>",
            esc(label),
            esc(rel)
        )
    }
}

fn sev_class(s: Sev) -> &'static str {
    s.name()
}

pub fn finding_card(a: &mut Assets, f: &Finding) -> String {
    let mut h = format!(
        "<article class=\"finding {}\" id=\"{}\" data-id=\"{}\"><header><span class=\"sev {}\">{}</span><code class=\"fid\">{}</code></header><h3>{}</h3>",
        sev_class(f.severity),
        esc(&f.id),
        esc(&f.id),
        sev_class(f.severity),
        f.severity.name(),
        esc(&f.id),
        esc(&f.title)
    );
    h += &format!("<p class=\"rule\"><b>Rule</b> {}</p>", esc(&f.rule));
    if !f.detail.is_empty() {
        h += &format!("<p class=\"detail\">{}</p>", esc(&f.detail));
    }
    if let Value::Object(m) = &f.numbers {
        if !m.is_empty() {
            h += "<dl class=\"nums\">";
            for (k, v) in m {
                let t = match v {
                    Value::Number(n) => n.as_f64().map_or(v.to_string(), |x| {
                        if x.fract() == 0.0 {
                            format!("{x:.0}")
                        } else {
                            format!("{x:.2}")
                        }
                    }),
                    Value::String(s) => s.clone(),
                    _ => v.to_string(),
                };
                h += &format!("<dt>{}</dt><dd>{}</dd>", esc(k), esc(&t));
            }
            h += "</dl>";
        }
    }
    h += "<div class=\"media\">";
    for s in &f.shots {
        let ov: Vec<_> = s
            .overlays
            .iter()
            .map(|o| (o.col, o.row, o.w, o.h, o.label.clone()))
            .collect();
        h += &a.shot(&s.path, &s.caption, &ov);
    }
    if let Some(c) = &f.chart {
        h += &svg_chart(c);
    }
    for c in &f.clips {
        h += &a.audio(&c.path, &c.label);
    }
    h += "</div></article>";
    h
}

pub fn table(head: &[&str], rows: &[Vec<String>]) -> String {
    let mut t = String::from("<div class=\"tablewrap\"><table><thead><tr>");
    for h in head {
        t += &format!("<th>{}</th>", esc(h));
    }
    t += "</tr></thead><tbody>";
    for r in rows {
        t += "<tr>";
        for c in r {
            t += &format!("<td>{c}</td>");
        }
        t += "</tr>";
    }
    t + "</tbody></table></div>"
}

pub const CSS: &str = r#"
:root{--bg:#f6f5f1;--panel:#fff;--ink:#1c1c1e;--muted:#5f5e58;--line:#d9d6cc;--accent:#8a5a00;--high:#b3261e;--medium:#a8610a;--low:#2f6f8f;--info:#667;--c1:#1c1c1e;--c2:#c9710b;--c3:#2b7bb9;--bezel:#050505;color-scheme:light dark}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#121214;--panel:#1c1c20;--ink:#ecebe6;--muted:#a09e96;--line:#2e2e34;--accent:#e7c75f;--high:#f2877e;--medium:#e8b36a;--low:#7fc0e0;--info:#a0a0b0;--c1:#ecebe6;--c2:#f0a95a;--c3:#7fc0e0}}
:root[data-theme=dark]{--bg:#121214;--panel:#1c1c20;--ink:#ecebe6;--muted:#a09e96;--line:#2e2e34;--accent:#e7c75f;--high:#f2877e;--medium:#e8b36a;--low:#7fc0e0;--info:#a0a0b0;--c1:#ecebe6;--c2:#f0a95a;--c3:#7fc0e0}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif;padding:24px max(16px,env(safe-area-inset-right)) 64px max(16px,env(safe-area-inset-left))}
main{max-width:1100px;margin:0 auto;display:grid;grid-template-columns:minmax(0,1fr);gap:28px}
main>*{min-width:0}
h1{font-size:clamp(26px,5vw,38px);margin:0;line-height:1.1}h2{font-size:22px;margin:0 0 8px}h3{font-size:16px;margin:6px 0}
p{margin:4px 0;max-width:75ch}.muted,.rule,.detail{color:var(--muted)}.rule b{color:var(--ink)}
code,.fid,dt,.mono{font-family:ui-monospace,Menlo,monospace;font-size:12px}
section{border-top:1px solid var(--line);padding-top:20px}
.cal{background:var(--panel);border:1px solid var(--line);border-radius:6px;padding:14px;display:grid;gap:10px;position:sticky;top:0;z-index:5}
.card-bar{height:18px;background:repeating-linear-gradient(90deg,var(--accent) 0 1px,transparent 1px 10px);border:1px solid var(--accent);max-width:100%}
label{font-size:13px;color:var(--muted);display:inline-flex;gap:8px;align-items:center;margin-right:16px}
input[type=number]{width:80px}
.summary{display:grid;grid-template-columns:repeat(auto-fit,minmax(120px,1fr));gap:10px}
.stat{background:var(--panel);border:1px solid var(--line);border-radius:6px;padding:10px}.stat b{display:block;font-size:26px}
.sev{display:inline-block;font:600 11px ui-monospace,monospace;text-transform:uppercase;padding:1px 8px;border-radius:3px;color:#fff;background:var(--info)}
.sev.high{background:var(--high)}.sev.medium{background:var(--medium)}.sev.low{background:var(--low)}
.finding{background:var(--panel);border:1px solid var(--line);border-left:5px solid var(--info);border-radius:6px;padding:12px 14px;margin:12px 0}
.finding.high{border-left-color:var(--high)}.finding.medium{border-left-color:var(--medium)}.finding.low{border-left-color:var(--low)}
.finding header{display:flex;gap:10px;align-items:center;flex-wrap:wrap}
.nums{display:grid;grid-template-columns:max-content 1fr;gap:2px 12px;margin:8px 0}.nums dt{color:var(--muted)}.nums dd{margin:0;font:12px ui-monospace,monospace;overflow-wrap:anywhere}
.media{display:flex;gap:18px;flex-wrap:wrap;align-items:flex-start}
.shot{margin:0}.pair{display:flex;gap:16px;align-items:flex-end;flex-wrap:wrap}
.bezel{position:relative;background:var(--bezel);padding:4px;border-radius:3px;line-height:0;display:inline-block}
.bezel img{image-rendering:pixelated;image-rendering:crisp-edges;display:block;max-width:100%;height:auto}
.bezel.real img{width:calc(var(--mm,23.4)*var(--ppm,3.78)*1px)}
.ov{position:absolute;outline:2px solid #ff3b30;background:rgba(255,59,48,.18);pointer-events:auto}
.bezel.real .ov,.bezel.one .ov{margin:4px 0 0 4px}
.scale,figcaption{font:11px ui-monospace,monospace;color:var(--muted);line-height:1.4;max-width:260px}
.chartfig{margin:0}.chart{width:380px;max-width:100%;background:var(--bg);border:1px solid var(--line);border-radius:4px}
.chart .grid{stroke:var(--line);stroke-width:1}.chart .tick{font:9px ui-monospace,monospace;fill:var(--muted)}
.chartfig figcaption i{display:inline-block;width:10px;height:3px;margin:0 4px 2px 8px}
.clip{display:grid;gap:2px;font-size:12px;color:var(--muted)}audio{height:32px;max-width:100%}
.tablewrap{overflow-x:auto}table{border-collapse:collapse;font-size:13px;min-width:100%}
th,td{border-bottom:1px solid var(--line);padding:4px 8px;text-align:left;vertical-align:top}th{color:var(--muted);font-weight:600}
td.num{text-align:right;font-variant-numeric:tabular-nums}.bad{color:var(--high);font-weight:600}.ok{color:var(--muted)}
.strip{display:flex;gap:12px;overflow-x:auto;padding:6px 0 10px}
.step{flex:0 0 auto;width:220px;background:var(--panel);border:1px solid var(--line);border-radius:6px;padding:8px;display:grid;gap:4px;align-content:start}
.step .bezel img{width:200px}.step .st{font:11px ui-monospace,monospace}
.step.nyp{opacity:.75;border-style:dashed}.step.dead{border-color:var(--high)}
details>summary{cursor:pointer;color:var(--muted)}
@media (max-width:600px){.bezel.real img{width:calc(var(--mm,23.4)*var(--ppm,3.78)*1px)}}
"#;

pub const JS: &str = r#"
(function(){
var root=document.documentElement;
function save(k,v){try{localStorage.setItem(k,v)}catch(e){}}
function load(k,d){try{return localStorage.getItem(k)||d}catch(e){return d}}
var ppm=document.getElementById('ppm'),mm=document.getElementById('mmw'),bar=document.getElementById('cardbar'),ro=document.getElementById('readout');
function apply(){root.style.setProperty('--ppm',ppm.value);root.style.setProperty('--mm',mm.value);bar.style.width=(85.6*ppm.value)+'px';ro.textContent=(+ppm.value).toFixed(2)+' px/mm, screen '+(mm.value*ppm.value).toFixed(0)+' px wide';save('ergo-ppm',ppm.value);save('ergo-mm',mm.value)}
ppm.value=load('ergo-ppm','3.78');mm.value=load('ergo-mm','23.4');ppm.oninput=mm.oninput=apply;apply();
document.querySelectorAll('[data-img]').forEach(function(i){i.src=IMG[i.dataset.img]});
document.querySelectorAll('[data-aud]').forEach(function(a){a.src=AUD[a.dataset.aud]});
})();
"#;
