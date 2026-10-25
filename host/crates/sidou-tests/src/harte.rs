// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Tom Harte's SingleStepTests for the 6502 (`6502/v1`, MIT): one JSON file per opcode, each
//! test an initial and a final state (registers and the memory it touches) and the bus's
//! cycles (address, value, read or write). The converter keeps the first tests of each file in
//! a compact binary, the subset SIDOU commits (`third_party/6502-tests/harte/`, see its
//! VENDORED.md); the reader reads that binary, and the JSON for the full `--ignored` run.
//!
//! The binary, little-endian: a `u32` count, then per test the initial and the final state
//! (`pc: u16, s, a, x, y, p: u8, n: u16`, then `n` times `address: u16, value: u8`) and the
//! cycles (`n: u16`, then `n` times `address: u16, value: u8, read: u8`: 1 a read, 0 a write).
//! It is the layout of the study's `harte2bin.py`.

use std::path::Path;

/// A CPU state: the registers and the memory cells the test sets or checks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub pc: u16,
    pub s: u8,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub p: u8,
    pub ram: Vec<(u16, u8)>,
}

/// One bus cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cycle {
    pub addr: u16,
    pub value: u8,
    pub read: bool,
}

/// One test: a single instruction from `initial` to `fin`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Test {
    pub name: String,
    pub initial: State,
    pub fin: State,
    pub cycles: Vec<Cycle>,
}

/// The first `count` tests of an opcode's JSON file (all of them for `usize::MAX`).
pub fn parse_json(text: &str, count: usize) -> Result<Vec<Test>, String> {
    let mut p = Json {
        s: text.as_bytes(),
        at: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.at != p.s.len() {
        return Err(format!("text after the list at byte {}", p.at));
    }
    let Value::List(list) = v else {
        return Err("not a list of tests".into());
    };
    list.iter().take(count).map(json_test).collect()
}

/// A JSON value, as much of JSON as the tests use: no escapes in strings, no fractions.
#[derive(Debug)]
enum Value {
    Num(u64),
    Str(String),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    fn get(&self, key: &str) -> Result<&Value, String> {
        match self {
            Value::Object(fields) => fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .ok_or_else(|| format!("no \"{key}\"")),
            _ => Err(format!("\"{key}\": not an object")),
        }
    }
    fn list(&self, what: &str) -> Result<&[Value], String> {
        match self {
            Value::List(l) => Ok(l),
            _ => Err(format!("{what}: not a list")),
        }
    }
    fn at(&self, i: usize, what: &str) -> Result<&Value, String> {
        self.list(what)?
            .get(i)
            .ok_or_else(|| format!("{what}: too short"))
    }
    fn num<T: TryFrom<u64>>(&self, what: &str) -> Result<T, String> {
        match self {
            Value::Num(n) => T::try_from(*n).map_err(|_| format!("{what}: {n} out of range")),
            _ => Err(format!("{what}: not a number")),
        }
    }
}

struct Json<'a> {
    s: &'a [u8],
    at: usize,
}

impl Json<'_> {
    fn ws(&mut self) {
        while self.at < self.s.len() && self.s[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }
    fn peek(&mut self) -> Result<u8, String> {
        self.ws();
        self.s
            .get(self.at)
            .copied()
            .ok_or_else(|| "unexpected end".into())
    }
    fn eat(&mut self, b: u8) -> Result<(), String> {
        if self.peek()? != b {
            return Err(format!("'{}' expected at byte {}", b as char, self.at));
        }
        self.at += 1;
        Ok(())
    }
    /// The items of a list or an object up to `close`, each read by `item`.
    fn items(
        &mut self,
        close: u8,
        mut item: impl FnMut(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.peek()? == close {
            self.at += 1;
            return Ok(());
        }
        loop {
            item(self)?;
            if self.peek()? == close {
                self.at += 1;
                return Ok(());
            }
            self.eat(b',')?;
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let start = self.at;
        while self.at < self.s.len() && self.s[self.at] != b'"' {
            if self.s[self.at] == b'\\' {
                return Err(format!("an escape at byte {}", self.at));
            }
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.s[start..self.at]).map_err(|e| e.to_string())?;
        self.eat(b'"')?;
        Ok(text.to_string())
    }
    fn value(&mut self) -> Result<Value, String> {
        match self.peek()? {
            b'[' => {
                self.at += 1;
                let mut list = Vec::new();
                self.items(b']', |p| {
                    list.push(p.value()?);
                    Ok(())
                })?;
                Ok(Value::List(list))
            }
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.items(b'}', |p| {
                    let key = p.string()?;
                    p.eat(b':')?;
                    fields.push((key, p.value()?));
                    Ok(())
                })?;
                Ok(Value::Object(fields))
            }
            b'"' => Ok(Value::Str(self.string()?)),
            b'0'..=b'9' => {
                let mut n = 0u64;
                while let Some(d) = self.s.get(self.at).filter(|d| d.is_ascii_digit()) {
                    n = n
                        .checked_mul(10)
                        .and_then(|n| n.checked_add(u64::from(d - b'0')))
                        .ok_or("a number too large")?;
                    self.at += 1;
                }
                Ok(Value::Num(n))
            }
            b => Err(format!("unexpected '{}' at byte {}", b as char, self.at)),
        }
    }
}

fn json_state(v: &Value) -> Result<State, String> {
    let ram = v
        .get("ram")?
        .list("ram")?
        .iter()
        .map(|cell| {
            Ok((
                cell.at(0, "ram")?.num("ram address")?,
                cell.at(1, "ram")?.num("ram value")?,
            ))
        })
        .collect::<Result<_, String>>()?;
    Ok(State {
        pc: v.get("pc")?.num("pc")?,
        s: v.get("s")?.num("s")?,
        a: v.get("a")?.num("a")?,
        x: v.get("x")?.num("x")?,
        y: v.get("y")?.num("y")?,
        p: v.get("p")?.num("p")?,
        ram,
    })
}

fn json_test(v: &Value) -> Result<Test, String> {
    let cycles = v
        .get("cycles")?
        .list("cycles")?
        .iter()
        .map(|c| {
            let read = match c.at(2, "cycle")? {
                Value::Str(k) if k == "read" => true,
                Value::Str(k) if k == "write" => false,
                k => return Err(format!("cycle kind: {k:?}")),
            };
            Ok(Cycle {
                addr: c.at(0, "cycle")?.num("cycle address")?,
                value: c.at(1, "cycle")?.num("cycle value")?,
                read,
            })
        })
        .collect::<Result<_, String>>()?;
    let name = match v.get("name")? {
        Value::Str(s) => s.clone(),
        _ => return Err("name: not a string".into()),
    };
    Ok(Test {
        name,
        initial: json_state(v.get("initial")?)?,
        fin: json_state(v.get("final")?)?,
        cycles,
    })
}

fn put_state(out: &mut Vec<u8>, s: &State) {
    out.extend_from_slice(&s.pc.to_le_bytes());
    out.extend_from_slice(&[s.s, s.a, s.x, s.y, s.p]);
    out.extend_from_slice(&(s.ram.len() as u16).to_le_bytes());
    for &(addr, value) in &s.ram {
        out.extend_from_slice(&addr.to_le_bytes());
        out.push(value);
    }
}

/// The binary form of `tests` (the module's layout).
pub fn to_bin(tests: &[Test]) -> Vec<u8> {
    let mut out = (tests.len() as u32).to_le_bytes().to_vec();
    for t in tests {
        put_state(&mut out, &t.initial);
        put_state(&mut out, &t.fin);
        out.extend_from_slice(&(t.cycles.len() as u16).to_le_bytes());
        for c in &t.cycles {
            out.extend_from_slice(&c.addr.to_le_bytes());
            out.extend_from_slice(&[c.value, u8::from(c.read)]);
        }
    }
    out
}

struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        if self.0.len() < n {
            return Err("truncated".into());
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn state(&mut self) -> Result<State, String> {
        let pc = self.u16()?;
        let r = self.take(5)?;
        let (s, a, x, y, p) = (r[0], r[1], r[2], r[3], r[4]);
        let n = self.u16()?;
        let ram = (0..n)
            .map(|_| Ok((self.u16()?, self.u8()?)))
            .collect::<Result<_, String>>()?;
        Ok(State {
            pc,
            s,
            a,
            x,
            y,
            p,
            ram,
        })
    }
}

/// The tests of a binary file (the module's layout); the names are not kept.
pub fn from_bin(bin: &[u8]) -> Result<Vec<Test>, String> {
    let mut b = Bytes(bin);
    let count = u32::from_le_bytes(b.take(4)?.try_into().unwrap());
    let mut tests = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let initial = b.state()?;
        let fin = b.state()?;
        let n = b.u16()?;
        let cycles = (0..n)
            .map(|_| {
                let addr = b.u16()?;
                let value = b.u8()?;
                Ok(Cycle {
                    addr,
                    value,
                    read: b.u8()? != 0,
                })
            })
            .collect::<Result<_, String>>()?;
        tests.push(Test {
            name: String::new(),
            initial,
            fin,
            cycles,
        });
    }
    if !b.0.is_empty() {
        return Err(format!("{} bytes after the last test", b.0.len()));
    }
    Ok(tests)
}

/// Converts the first `count` tests of each of the 256 opcode files in `src` (`00.json` to
/// `ff.json`) into `dst/<op>.bin`. Returns the bytes written.
pub fn convert(src: &Path, dst: &Path, count: usize) -> Result<usize, String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    let mut total = 0;
    for op in 0..=255u8 {
        let path = src.join(format!("{op:02x}.json"));
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let tests = parse_json(&text, count).map_err(|e| format!("{}: {e}", path.display()))?;
        let bin = to_bin(&tests);
        total += bin.len();
        let out = dst.join(format!("{op:02x}.bin"));
        std::fs::write(&out, bin).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(total)
}
