// Minimal Windows registry hive (regf) reader/writer and BCD editor.
// Used by the installer to add the "Cheip OS" entry to the Lumia boot menu
// without bcdedit: parse the hive into a tree, edit it, write a fresh hive.

const HBIN = 0x1000;
const enc16 = (s) => {
  const b = new Uint8Array(s.length * 2);
  for (let i = 0; i < s.length; i++) { const c = s.charCodeAt(i); b[i * 2] = c & 255; b[i * 2 + 1] = c >> 8; }
  return b;
};
const dec16 = (b) => { let s = ""; for (let i = 0; i + 1 < b.length; i += 2) s += String.fromCharCode(b[i] | (b[i + 1] << 8)); return s; };
const latin1 = (b) => { let s = ""; for (const c of b) s += String.fromCharCode(c); return s; };
const isLatin1 = (s) => [...s].every((c) => c.charCodeAt(0) < 256);

export const REG_SZ = 1, REG_BINARY = 3, REG_DWORD = 4, REG_MULTI_SZ = 7;

export function parseHive(buf) {
  const u8 = new Uint8Array(buf);
  const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
  if (latin1(u8.subarray(0, 4)) !== "regf") throw new Error("Not a registry hive");
  const rootOff = dv.getUint32(0x24, true);
  const cell = (off) => {
    const p = HBIN + off;
    const size = -dv.getInt32(p, true);
    if (size <= 0) throw new Error("Free cell referenced at " + off);
    return { p: p + 4, len: size - 4 };
  };
  const sig = (p) => latin1(u8.subarray(p, p + 2));
  const bytes = (off, n) => { const c = cell(off); return u8.slice(c.p, c.p + n); };

  function subkeyOffsets(off, out = []) {
    if (off === 0xffffffff) return out;
    const { p } = cell(off);
    const s = sig(p), n = dv.getUint16(p + 2, true);
    if (s === "lf" || s === "lh") for (let i = 0; i < n; i++) out.push(dv.getUint32(p + 4 + i * 8, true));
    else if (s === "li") for (let i = 0; i < n; i++) out.push(dv.getUint32(p + 4 + i * 4, true));
    else if (s === "ri") for (let i = 0; i < n; i++) subkeyOffsets(dv.getUint32(p + 4 + i * 4, true), out);
    else throw new Error("Unknown subkey list " + s);
    return out;
  }

  function value(off) {
    const { p } = cell(off);
    if (sig(p) !== "vk") throw new Error("Bad vk");
    const nlen = dv.getUint16(p + 2, true);
    let dlen = dv.getUint32(p + 4, true);
    const doff = dv.getUint32(p + 8, true);
    const type = dv.getUint32(p + 12, true);
    const flags = dv.getUint16(p + 16, true);
    const nb = u8.subarray(p + 20, p + 20 + nlen);
    const name = flags & 1 ? latin1(nb) : dec16(nb);
    let data;
    if (dlen & 0x80000000) { dlen &= 0x7fffffff; data = u8.slice(p + 8, p + 8 + dlen); }
    else if (dlen === 0) data = new Uint8Array(0);
    else {
      const c = cell(doff);
      if (dlen > 16344 && sig(c.p) === "db") {
        const segs = dv.getUint16(c.p + 2, true), list = cell(dv.getUint32(c.p + 4, true)).p;
        data = new Uint8Array(dlen); let at = 0;
        for (let i = 0; i < segs && at < dlen; i++) {
          const sc = cell(dv.getUint32(list + i * 4, true));
          const take = Math.min(16344, dlen - at, sc.len);
          data.set(u8.subarray(sc.p, sc.p + take), at); at += take;
        }
      } else data = u8.slice(c.p, c.p + dlen);
    }
    return { name, type, data };
  }

  function key(off) {
    const { p } = cell(off);
    if (sig(p) !== "nk") throw new Error("Bad nk at " + off);
    const flags = dv.getUint16(p + 2, true);
    const subOff = dv.getUint32(p + 28, true);
    const nval = dv.getUint32(p + 36, true), valOff = dv.getUint32(p + 40, true);
    const skOff = dv.getUint32(p + 44, true), clsOff = dv.getUint32(p + 48, true);
    const nlen = dv.getUint16(p + 72, true), clen = dv.getUint16(p + 74, true);
    const nb = u8.subarray(p + 76, p + 76 + nlen);
    const name = flags & 0x20 ? latin1(nb) : dec16(nb);
    let sk = null;
    if (skOff !== 0xffffffff) {
      const c = cell(skOff);
      const dsz = dv.getUint32(c.p + 16, true);
      sk = u8.slice(c.p + 20, c.p + 20 + dsz);
    }
    const values = [];
    if (nval && valOff !== 0xffffffff) {
      const { p: vp } = cell(valOff);
      for (let i = 0; i < nval; i++) values.push(value(dv.getUint32(vp + i * 4, true)));
    }
    const subkeys = subkeyOffsets(subOff).map(key);
    return { name, flags: flags & ~0x20, cls: clsOff !== 0xffffffff && clen ? bytes(clsOff, clen) : null, sk, values, subkeys };
  }

  return { header: u8.slice(0, 0x1000), root: key(rootOff) };
}

const up = (s) => s.toUpperCase();
const cmpName = (a, b) => (up(a.name) < up(b.name) ? -1 : up(a.name) > up(b.name) ? 1 : 0);
const lhHash = (name) => { let h = 0; for (const c of up(name)) h = (Math.imul(h, 37) + c.charCodeAt(0)) >>> 0; return h; };
const al8 = (n) => (n + 7) & ~7;

export function writeHive(hive) {
  const cells = []; // {size, write(dv,u8,p)} ; p = absolute payload position
  const alloc = (len, write) => { const c = { size: al8(len + 4), write, off: 0 }; cells.push(c); return c; };

  // Security descriptors: one sk cell per distinct descriptor, linked in a ring
  const sks = new Map();
  const skFor = (desc) => {
    if (!desc) return null;
    const k = Array.from(desc).join(",");
    if (!sks.has(k)) {
      const s = { desc, refs: 0 };
      s.cell = alloc(20 + desc.length, (dv, u8, p) => {
        u8.set([0x73, 0x6b], p);
        dv.setUint32(p + 4, s.next.cell.off, true);
        dv.setUint32(p + 8, s.prev.cell.off, true);
        dv.setUint32(p + 12, s.refs, true);
        dv.setUint32(p + 16, desc.length, true);
        u8.set(desc, p + 20);
      });
      sks.set(k, s);
    }
    const s = sks.get(k); s.refs++; return s;
  };

  function addValue(v) {
    const comp = isLatin1(v.name);
    const nb = comp ? Uint8Array.from(v.name, (c) => c.charCodeAt(0)) : enc16(v.name);
    if (v.data.length > 16344) throw new Error("Value too large: " + v.name);
    const dcell = v.data.length > 4 ? alloc(v.data.length, (dv, u8, p) => u8.set(v.data, p)) : null;
    return alloc(20 + nb.length, (dv, u8, p) => {
      u8.set([0x76, 0x6b], p);
      dv.setUint16(p + 2, nb.length, true);
      if (dcell) { dv.setUint32(p + 4, v.data.length, true); dv.setUint32(p + 8, dcell.off, true); }
      else { dv.setUint32(p + 4, (v.data.length | 0x80000000) >>> 0, true); u8.set(v.data, p + 8); }
      dv.setUint32(p + 12, v.type, true);
      dv.setUint16(p + 16, comp ? 1 : 0, true);
      u8.set(nb, p + 20);
    });
  }

  function addKey(k, parent, isRoot) {
    const comp = isLatin1(k.name);
    const nb = comp ? Uint8Array.from(k.name, (c) => c.charCodeAt(0)) : enc16(k.name);
    const me = alloc(76 + nb.length, null);
    const sk = skFor(k.sk || parent?.sk);
    k.sk = k.sk || parent?.sk;
    const subs = [...k.subkeys].sort(cmpName);
    const subCells = subs.map((s) => addKey(s, { cell: me, sk: k.sk }, false));
    const list = subs.length ? alloc(4 + subs.length * 8, (dv, u8, p) => {
      u8.set([0x6c, 0x68], p); dv.setUint16(p + 2, subs.length, true);
      subs.forEach((s, i) => { dv.setUint32(p + 4 + i * 8, subCells[i].off, true); dv.setUint32(p + 8 + i * 8, lhHash(s.name), true); });
    }) : null;
    const vcells = k.values.map(addValue);
    const vlist = vcells.length ? alloc(vcells.length * 4, (dv, u8, p) => vcells.forEach((c, i) => dv.setUint32(p + i * 4, c.off, true))) : null;
    const ccell = k.cls ? alloc(k.cls.length, (dv, u8, p) => u8.set(k.cls, p)) : null;
    const maxSub = Math.max(0, ...subs.map((s) => s.name.length * 2));
    const maxVN = Math.max(0, ...k.values.map((v) => v.name.length * 2));
    const maxVD = Math.max(0, ...k.values.map((v) => v.data.length));
    me.write = (dv, u8, p) => {
      u8.set([0x6e, 0x6b], p);
      dv.setUint16(p + 2, (k.flags & ~0x20) | (isRoot ? 0x0c : 0) | (comp ? 0x20 : 0), true);
      const t = BigInt(Date.now()) * 10000n + 116444736000000000n;
      dv.setBigUint64(p + 4, t, true);
      dv.setUint32(p + 16, parent ? parent.cell.off : 0, true);
      dv.setUint32(p + 20, subs.length, true);
      dv.setUint32(p + 24, 0, true);
      dv.setUint32(p + 28, list ? list.off : 0xffffffff, true);
      dv.setUint32(p + 32, 0xffffffff, true);
      dv.setUint32(p + 36, vcells.length, true);
      dv.setUint32(p + 40, vlist ? vlist.off : 0xffffffff, true);
      dv.setUint32(p + 44, sk ? sk.cell.off : 0xffffffff, true);
      dv.setUint32(p + 48, ccell ? ccell.off : 0xffffffff, true);
      dv.setUint32(p + 52, maxSub, true);
      dv.setUint32(p + 56, 0, true);
      dv.setUint32(p + 60, maxVN, true);
      dv.setUint32(p + 64, maxVD, true);
      dv.setUint32(p + 68, 0, true);
      dv.setUint16(p + 72, nb.length, true);
      dv.setUint16(p + 74, k.cls ? k.cls.length : 0, true);
      u8.set(nb, p + 76);
    };
    return me;
  }

  const root = addKey(hive.root, null, true);
  const ring = [...sks.values()];
  ring.forEach((s, i) => { s.next = ring[(i + 1) % ring.length]; s.prev = ring[(i - 1 + ring.length) % ring.length]; });

  // Layout: pack cells into 4 KiB hbins, a cell never crosses a bin boundary
  const bins = []; let cur = null, pos = 0;
  for (const c of cells) {
    if (!cur || pos + c.size > cur.start + cur.size) {
      const size = Math.max(0x1000, Math.ceil((c.size + 32) / 0x1000) * 0x1000);
      const start = cur ? cur.start + cur.size : 0;
      cur = { start, size }; bins.push(cur); pos = start + 32;
    }
    c.off = pos; pos += c.size; cur.end = pos;
  }
  const dataSize = cur.start + cur.size;
  const out = new Uint8Array(HBIN + dataSize);
  const dv = new DataView(out.buffer);
  out.set(hive.header.subarray(0, HBIN));
  for (const b of bins) {
    const p = HBIN + b.start;
    out.set([0x68, 0x62, 0x69, 0x6e], p);
    dv.setUint32(p + 4, b.start, true);
    dv.setUint32(p + 8, b.size, true);
    if (b.end < b.start + b.size) dv.setInt32(HBIN + b.end, b.start + b.size - b.end, true); // free tail
  }
  for (const c of cells) { dv.setInt32(HBIN + c.off, -c.size, true); c.write(dv, out, HBIN + c.off + 4); }

  const seq = dv.getUint32(4, true) + 1;
  dv.setUint32(4, seq, true); dv.setUint32(8, seq, true);
  dv.setBigUint64(12, BigInt(Date.now()) * 10000n + 116444736000000000n, true);
  dv.setUint32(0x24, root.off, true);
  dv.setUint32(0x28, dataSize, true);
  let x = 0; for (let i = 0; i < 0x1fc; i += 4) x ^= dv.getUint32(i, true);
  x >>>= 0; if (x === 0xffffffff) x = 0xfffffffe; if (x === 0) x = 1;
  dv.setUint32(0x1fc, x, true);
  return out;
}

// ---- BCD ----
export const BOOTMGR = "{9dea862c-5cdd-4e70-acc1-f32b344d4795}";
const E = { path: "12000002", desc: "12000004", order: "24000001", def: "23000003", menu: "26000020", timeout: "25000004", noint: "16000048", testsign: "16000049" };

const child = (k, n) => k.subkeys.find((s) => up(s.name) === up(n));
const ensure = (k, n) => { let c = child(k, n); if (!c) { c = { name: n, flags: 0, cls: null, sk: null, values: [], subkeys: [] }; k.subkeys.push(c); } return c; };
const elem = (obj, id) => { const e = child(child(obj, "Elements") || { subkeys: [] }, id); return e && e.values.find((v) => v.name === "Element"); };
const setElem = (obj, id, type, data) => {
  const e = ensure(ensure(obj, "Elements"), id);
  const v = e.values.find((x) => x.name === "Element");
  if (v) { v.type = type; v.data = data; } else e.values.push({ name: "Element", type, data });
};
const sz = (s) => enc16(s + "\0");
const multi = (list) => enc16(list.join("\0") + "\0\0");
const readMulti = (d) => dec16(d).split("\0").filter(Boolean);
const readSz = (d) => dec16(d).replace(/\0+$/, "");
const deepClone = (k) => ({ ...k, values: k.values.map((v) => ({ ...v, data: v.data.slice() })), subkeys: k.subkeys.map(deepClone) });

export function bcdEntries(hive) {
  const objs = child(hive.root, "Objects");
  if (!objs) throw new Error("This is not a BCD store");
  return objs.subkeys.map((o) => ({
    id: o.name,
    desc: elem(o, E.desc) ? readSz(elem(o, E.desc).data) : "",
    path: elem(o, E.path) ? readSz(elem(o, E.path).data) : "",
  }));
}

// Adds (or refreshes) the "Cheip OS" entry. Returns its GUID.
export function bcdAddCheip(hive, { path = "\\Windows\\System32\\Boot\\cheipshim.efi", title = "Cheip OS", timeout = 5 } = {}) {
  const objs = child(hive.root, "Objects");
  const mgr = child(objs, BOOTMGR);
  if (!mgr) throw new Error("Boot manager entry not found in BCD");
  let entry = objs.subkeys.find((o) => elem(o, E.desc) && readSz(elem(o, E.desc).data) === title);
  if (!entry) {
    const src = objs.subkeys.find((o) => elem(o, E.path) && /resetphone/i.test(readSz(elem(o, E.path).data)))
      || objs.subkeys.find((o) => elem(o, E.path) && /\.efi$/i.test(readSz(elem(o, E.path).data)) && o.name !== BOOTMGR);
    if (!src) throw new Error("No boot application entry to copy in BCD");
    entry = deepClone(src);
    entry.name = "{" + crypto.randomUUID() + "}";
    objs.subkeys.push(entry);
  }
  setElem(entry, E.path, REG_SZ, sz(path));
  setElem(entry, E.desc, REG_SZ, sz(title));
  setElem(entry, E.noint, REG_BINARY, Uint8Array.of(1));
  setElem(mgr, E.noint, REG_BINARY, Uint8Array.of(1));
  const order = elem(mgr, E.order) ? readMulti(elem(mgr, E.order).data) : [];
  if (!order.length && elem(mgr, E.def)) order.push(readSz(elem(mgr, E.def).data));
  if (!order.some((g) => up(g) === up(entry.name))) order.push(entry.name);
  setElem(mgr, E.order, REG_MULTI_SZ, multi(order));
  setElem(mgr, E.menu, REG_BINARY, Uint8Array.of(1));
  const t = new Uint8Array(8); new DataView(t.buffer).setUint32(0, timeout, true);
  setElem(mgr, E.timeout, REG_BINARY, t);
  return entry.name;
}
