// Cheip OS installer: dispositivi, avanzamento passi, link alle release, console USB (fastboot)
import { FastbootDevice, setDebugLevel } from "./vendor/fastboot.min.mjs";
import { parseHive, writeHive, bcdAddCheip, bcdEntries } from "./bcd.mjs";

// Repository pubblico che ospita le release scaricabili
const REPO = "cheipstudio-cmyk/cheip-os";

const DEVICES = [
  { id: "735", name: "Lumia 735", code: "RM-1038 · superman-lte", soc: "MSM8926", status: "ok", label: "Experimental", fill: "#82d5c7" },
  { id: "730", name: "Lumia 730 Dual SIM", code: "RM-1040 · superman", soc: "MSM8226", status: "soon", fill: "#f2bd7b" },
  { id: "640", name: "Lumia 640", code: "RM-1072/1077 · dempsey", soc: "MSM8926", status: "soon", fill: "#c4b5fd" },
  { id: "640xl", name: "Lumia 640 XL", code: "RM-1062/1065 · makepeace", soc: "MSM8226", status: "soon", fill: "#f9a8d4" },
  { id: "630", name: "Lumia 630 / 635", code: "RM-976/974 · moneypenny", soc: "MSM8226", status: "soon", fill: "#93c5fd" },
  { id: "830", name: "Lumia 830", code: "RM-984 · tesla", soc: "MSM8926", status: "soon", fill: "#86efac" },
];

const $ = (s) => document.querySelector(s);
const store = {
  get: (k, d) => { try { return JSON.parse(localStorage.getItem(k)) ?? d; } catch { return d; } },
  set: (k, v) => { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
};

// ---------- Dispositivi ----------
function renderDevices() {
  const list = $("#devlist");
  list.innerHTML = "";
  for (const d of DEVICES) {
    const el = document.createElement("div");
    el.className = "dev" + (d.status === "ok" ? (d.id === "735" ? " sel" : "") : " soon");
    el.innerHTML = `
      <svg class="shape" viewBox="0 0 100 100"><use href="#cookie" fill="${d.fill}"/></svg>
      <h3>${d.name}</h3>
      <div class="code">${d.code} · ${d.soc}</div>
      <span class="chip ${d.status === "ok" ? "ok" : "soon"}">${d.status === "ok" ? d.label : "Coming soon"}</span>`;
    if (d.status === "ok") el.addEventListener("click", () => location.hash = "#install");
    list.appendChild(el);
  }
}

// ---------- Passi ----------
function renderSteps() {
  const done = store.get("cheip-steps", {});
  document.querySelectorAll(".step").forEach((step) => {
    const n = step.dataset.step;
    const box = step.querySelector("input[type=checkbox]");
    box.checked = !!done[n];
    step.classList.toggle("done", box.checked);
    box.onchange = () => {
      done[n] = box.checked;
      store.set("cheip-steps", done);
      step.classList.toggle("done", box.checked);
    };
  });
}

// ---------- Release ----------
function human(b) {
  if (b > 1e9) return (b / 1e9).toFixed(2) + " GB";
  if (b > 1e6) return (b / 1e6).toFixed(1) + " MB";
  return Math.round(b / 1e3) + " KB";
}

async function loadRelease() {
  let rel = null;
  try {
    const r = await fetch(`https://api.github.com/repos/${REPO}/releases?per_page=10`);
    if (r.ok) rel = (await r.json()).find((x) => !x.draft && x.assets.length > 0);
  } catch {}
  const assets = rel ? rel.assets : [];
  document.querySelectorAll(".dl[data-asset], .dl[data-asset-match]").forEach((a) => {
    const name = a.dataset.asset;
    const re = a.dataset.assetMatch ? new RegExp(a.dataset.assetMatch) : null;
    const asset = assets.find((x) => (name && x.name.endsWith(name)) || (re && re.test(x.name)));
    if (asset) {
      a.href = asset.browser_download_url;
      a.removeAttribute("disabled");
    } else {
      a.setAttribute("disabled", "");
      a.href = "#";
    }
  });
}

// ---------- Image served by this site ----------
const OS = "os/";
let manifest = null;
async function loadManifest() {
  try {
    const r = await fetch(OS + "manifest.json", { cache: "no-cache" });
    if (r.ok) manifest = await r.json();
  } catch {}
  $("#img-info").textContent = manifest
    ? `Cheip OS ${manifest.version} · ${human(manifest.download)} to download, ${human(manifest.data)} written`
    : "The first public build is on its way.";
  return manifest;
}

const logTo = (el) => (t) => { el.textContent += t + "\n"; el.scrollTop = el.scrollHeight; };

// ---------- Step 3: boot menu on EFIESP (File System Access API) ----------
const espLog = logTo($("#esp-out"));

// Direct lookups (the PC's file system ignores case). Listing a drive root
// fails on "System Volume Information", so we never iterate unless needed.
async function dirPath(root, parts, create = false) {
  let d = root;
  for (const p of parts) {
    try { d = await d.getDirectoryHandle(p, { create }); }
    catch (e) { if (e.name === "NotFoundError" || e.name === "TypeMismatchError") return null; throw e; }
  }
  return d;
}
async function fileIn(dir, name) {
  try { return await dir.getFileHandle(name); }
  catch (e) { if (e.name === "NotFoundError" || e.name === "TypeMismatchError") return null; throw e; }
}
async function writeFile(dir, name, data) {
  const h = (await fileIn(dir, name)) || (await dir.getFileHandle(name, { create: true }));
  const w = await h.createWritable();
  await w.write(data);
  await w.close();
}
function saveCopy(name, data) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([data]));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 5000);
}
async function fetchBin(path) {
  const r = await fetch(OS + path, { cache: "no-cache" });
  if (!r.ok) throw new Error(`${path} is not on the site yet (${r.status})`);
  return new Uint8Array(await r.arrayBuffer());
}

async function pickEsp() {
  if (!("showDirectoryPicker" in window)) throw new Error("This browser can't write to the phone. Use Chrome or Edge on the PC.");
  let root = await window.showDirectoryPicker({ id: "efiesp", mode: "readwrite" });
  // Picked MainOS instead of its EFIESP folder: step inside
  const inner = await dirPath(root, ["EFIESP"]);
  if (inner && (await dirPath(inner, ["efi"]))) root = inner;
  const bootDir = await dirPath(root, ["efi", "Microsoft", "Boot"]);
  const bcdH = bootDir && (await fileIn(bootDir, "BCD"));
  const sysBoot = await dirPath(root, ["Windows", "System32", "Boot"]);
  if (!bcdH || !sysBoot) throw new Error(`"${root.name}" is not EFIESP: efi\\Microsoft\\Boot\\BCD is missing. Pick the EFIESP drive or the EFIESP folder inside MainOS.`);
  return { root, bootDir, bcdH, sysBoot };
}

function espState(ok, label) {
  $("#esp-dot").classList.toggle("on", ok);
  $("#esp-state").textContent = label;
}

$("#esp-install").addEventListener("click", () => installMenu(false));
$("#esp-default").addEventListener("click", () => installMenu(true));
async function installMenu(makeDefault) {
  try {
    const esp = await pickEsp();
    espLog(`EFIESP found (${esp.root.name}).`);
    const [shim, lk] = await Promise.all([fetchBin("boot/BootShim.efi"), fetchBin("boot/emmc_appsboot.mbn")]);
    const original = new Uint8Array(await (await esp.bcdH.getFile()).arrayBuffer());
    const hive = parseHive(original);
    if (!(await fileIn(esp.bootDir, "BCD.cheipbak"))) {
      await writeFile(esp.bootDir, "BCD.cheipbak", original);
      saveCopy("BCD-lumia-backup", original);
      espLog("Backup saved on the phone and in Downloads.");
    }
    await writeFile(esp.sysBoot, "cheipshim.efi", shim);
    await writeFile(esp.root, "emmc_appsboot.mbn", lk);
    espLog("Bootloader copied.");
    const id = bcdAddCheip(hive, { makeDefault });
    const bcd = writeHive(hive);
    parseHive(bcd); // sanity check before replacing the real one
    await writeFile(esp.bootDir, "BCD", bcd);
    espLog(`Boot menu ready: ${bcdEntries(hive).filter((e) => e.desc).map((e) => e.desc).join(", ")}.`);
    espLog(`Cheip OS entry ${id}. Starts by default: ${makeDefault ? "Cheip OS" : "Windows Phone"}. Go to step 4.`);
    espState(true, "Boot menu installed");
    markStep(3);
  } catch (e) {
    if (e.name === "AbortError") return;
    espLog("Error: " + (e.message || e));
    espState(false, "Not installed");
  }
}

$("#esp-remove").addEventListener("click", async () => {
  try {
    const esp = await pickEsp();
    const bak = await fileIn(esp.bootDir, "BCD.cheipbak");
    if (!bak) throw new Error("No backup on this phone: Cheip OS was never added here.");
    await writeFile(esp.bootDir, "BCD", new Uint8Array(await (await bak.getFile()).arrayBuffer()));
    for (const [dir, n] of [[esp.sysBoot, "cheipshim.efi"], [esp.root, "emmc_appsboot.mbn"], [esp.bootDir, "BCD.cheipbak"]])
      if (await fileIn(dir, n)) await dir.removeEntry(n);
    espLog("Original boot menu restored. The microSD keeps Cheip OS until you format it.");
    espState(false, "Removed");
  } catch (e) {
    if (e.name !== "AbortError") espLog("Error: " + (e.message || e));
  }
});

function markStep(n) {
  const done = store.get("cheip-steps", {});
  done[n] = true;
  store.set("cheip-steps", done);
  renderSteps();
}

// ---------- Step 4: install over USB (lk1st fastboot, WebUSB) ----------
const log = logTo($("#usb-out"));
const bar = $("#usb-bar");
const progress = (p) => { bar.style.display = "block"; bar.firstElementChild.style.width = (p * 100).toFixed(1) + "%"; };
let dev = null, sd = null, busy = false;

function setConnected(ok, label) {
  $("#usb-dot").classList.toggle("on", ok);
  $("#usb-state").textContent = label;
  for (const id of ["#usb-boot", "#usb-info", "#usb-reboot", "#usb-log", "#usb-klog"]) $(id).disabled = !ok || busy;
  $("#usb-install").disabled = !ok || busy || !sd || !manifest;
}

$("#usb-connect").addEventListener("click", async () => {
  if (!("usb" in navigator)) return log("This browser has no WebUSB. Open the page in Chrome or Edge on a PC.");
  try {
    setDebugLevel(0);
    dev = new FastbootDevice();
    await dev.connect();
    const product = await dev.getVariable("product").catch(() => "");
    sd = null;
    try {
      const r = await dev.runCommand("oem sd-info");
      const [blocks, bs] = r.text.trim().split(/\s+/).map(Number);
      if (blocks && bs) sd = { blocks, bs, bytes: blocks * bs };
    } catch {}
    if (!sd) {
      setConnected(true, `Connected${product ? " · " + product : ""}`);
      log("Bootloader found, but it can't see a microSD or it's an older bootloader. Repeat step 3, then put a card in the phone.");
      return;
    }
    await loadManifest();
    setConnected(true, `Connected · microSD ${human(sd.bytes)}`);
    log(`Cheip OS bootloader ready. microSD: ${human(sd.bytes)}.`);
    if (manifest && manifest.size > sd.bytes) log(`The card is too small: Cheip OS needs ${human(manifest.size)}.`);
  } catch (e) {
    setConnected(false, "Not connected");
    log("Connection failed: " + (e.message || e) + "\nThe phone must show the Cheip OS bootloader (step 4). If Android is missing from the list, unplug and plug the cable again.");
  }
});

async function sha256hex(buf) {
  return [...new Uint8Array(await crypto.subtle.digest("SHA-256", buf))].map((b) => b.toString(16).padStart(2, "0")).join("");
}
async function getChunk(c) {
  for (let attempt = 1; ; attempt++) {
    try {
      const r = await fetch(OS + c.file);
      if (!r.ok) throw new Error("HTTP " + r.status);
      const buf = await new Response(r.body.pipeThrough(new DecompressionStream("gzip"))).arrayBuffer();
      if (buf.byteLength !== c.length || (await sha256hex(buf)) !== c.sha256) throw new Error("corrupted download");
      return buf;
    } catch (e) {
      if (attempt >= 3) throw new Error(`${c.file}: ${e.message || e}`);
    }
  }
}

$("#usb-install").addEventListener("click", async () => {
  if (!dev || !sd || !manifest || busy) return;
  if (manifest.size > sd.bytes) return log("The microSD is too small for Cheip OS.");
  if (!confirm(`Everything on the ${human(sd.bytes)} microSD inside the phone will be erased and replaced by Cheip OS ${manifest.version}. Continue?`)) return;
  busy = true; setConnected(true, "Installing…");
  const total = manifest.data;
  let done = 0;
  const t0 = performance.now();
  try {
    let next = getChunk(manifest.chunks[0]);
    for (let i = 0; i < manifest.chunks.length; i++) {
      const c = manifest.chunks[i];
      const buf = await next;
      if (i + 1 < manifest.chunks.length) next = getChunk(manifest.chunks[i + 1]); // download ahead while writing
      await dev.upload("sd", buf, (p) => progress((done + p * c.length) / total));
      await dev.runCommand(`oem sd-write ${(c.offset / sd.bs).toString(16)}`);
      done += c.length;
      progress(done / total);
      const mbs = done / 1048576 / ((performance.now() - t0) / 1000);
      $("#usb-state").textContent = `Installing… ${Math.round((done / total) * 100)}% · ${mbs.toFixed(1)} MB/s`;
    }
    log(`Cheip OS ${manifest.version} written in ${Math.round((performance.now() - t0) / 1000)} s.`);
    busy = false;
    setConnected(true, "Installed");
    markStep(4);
    log("Restarting into Cheip OS…");
    await dev.reboot().catch(() => {});
    setConnected(false, "Restarting");
  } catch (e) {
    busy = false;
    setConnected(!!dev, "Install stopped");
    log("Install stopped: " + (e.message || e) + "\nNothing is broken: Windows Phone still starts. Reconnect and press Install again.");
  }
});

$("#usb-reboot").addEventListener("click", async () => { await dev.reboot().catch(() => {}); setConnected(false, "Restarting"); });

$("#usb-info").addEventListener("click", async () => {
  for (const v of ["product", "version-bootloader", "serialno", "max-download-size", "unlocked"]) {
    const val = await dev.getVariable(v).catch(() => "");
    log(`${v}: ${val || "?"}`);
  }
});

$("#usb-log").addEventListener("click", async () => {
  try {
    log("---- bootloader log ----\n" + (await fetchStaged(dev, "oem log")) + "\n------------------------");
  } catch (e) {
    log("Log failed: " + (e.message || e));
  }
});

$("#usb-klog").addEventListener("click", async () => {
  try {
    const t = await fetchStaged(dev, "oem ramoops console");
    log("---- last Linux log ----\n" + (t || "(empty: Linux did not start or the memory was cleared)") + "\n------------------------");
  } catch (e) {
    log("Linux log failed: " + (e.message || e));
  }
});

$("#usb-boot").addEventListener("click", () => $("#boot-file").click());
$("#boot-file").addEventListener("change", async (ev) => {
  const file = ev.target.files[0];
  if (!file || !dev) return;
  log(`Sending ${file.name} (${human(file.size)})…`);
  try {
    await dev.bootBlob(file, progress);
    log("Started from RAM: the phone disconnects now.");
    setConnected(false, "Booting");
  } catch (e) {
    log("Boot failed: " + (e.message || e));
  } finally {
    ev.target.value = "";
  }
});

// lk2nd "oem log" / "oem ramoops ..." stage their output: fetch it with "upload"
async function fetchStaged(d, cmd) {
  await d.runCommand(cmd);
  const enc = new TextEncoder();
  await d.device.transferOut(d.epOut, enc.encode("upload"));
  let head = "";
  for (;;) {
    const r = await d.device.transferIn(d.epIn, 64);
    head = new TextDecoder().decode(r.data);
    if (head.startsWith("INFO")) continue;
    break;
  }
  if (head.startsWith("FAIL")) return "";
  if (!head.startsWith("DATA")) throw new Error("unexpected reply " + head);
  const size = parseInt(head.slice(4, 12), 16);
  const out = new Uint8Array(size);
  let got = 0;
  while (got < size) {
    const r = await d.device.transferIn(d.epIn, Math.min(16384, size - got));
    out.set(new Uint8Array(r.data.buffer, r.data.byteOffset, r.data.byteLength), got);
    got += r.data.byteLength;
  }
  await d.device.transferIn(d.epIn, 64); // OKAY
  return new TextDecoder("utf-8", { fatal: false }).decode(out).replace(/\0+/g, "");
}

// Debug watch: the Cheip bootloader waits a few seconds for a host before
// booting. A phone this page already knows connects without a click.
async function captureLogs() {
  if (busy) return;
  busy = true;
  try {
    setDebugLevel(0);
    dev = new FastbootDevice();
    await dev.connect();
    const product = await dev.getVariable("product").catch(() => "");
    log(`Phone caught in the bootloader${product ? " (" + product + ")" : ""}. Reading logs…`);
    let all = "";
    for (const [title, cmd] of [["bootloader log", "oem log"], ["last Linux log", "oem ramoops console"], ["last Linux crash", "oem ramoops dump"]]) {
      try {
        const t = await fetchStaged(dev, cmd);
        all += `==== ${title} ====\n${t || "(empty)"}\n\n`;
        log(`---- ${title} ----\n${(t || "(empty)").slice(-6000)}\n------------------------`);
      } catch (e) {
        log(`${title} failed: ${e.message || e}`);
      }
    }
    if (all) saveCopy("cheip-logs.txt", new TextEncoder().encode(all));
    sd = null;
    try {
      const r = await dev.runCommand("oem sd-info");
      const [blocks, bs] = r.text.trim().split(/\s+/).map(Number);
      if (blocks && bs) sd = { blocks, bs, bytes: blocks * bs };
    } catch {}
    busy = false;
    setConnected(true, "Held in the bootloader (debug)");
  } catch (e) {
    busy = false;
    log("Debug capture failed: " + (e.message || e));
  }
}
if ("usb" in navigator) {
  $("#usb-watch").checked = store.get("cheip-watch", false);
  $("#usb-watch").addEventListener("change", (e) => store.set("cheip-watch", e.target.checked));
  navigator.usb.addEventListener("connect", () => { if ($("#usb-watch").checked) captureLogs(); });
  // Phone already sitting in the bootloader when the page loads
  navigator.usb.getDevices().then((ds) => { if (ds.length === 1 && $("#usb-watch").checked) captureLogs(); });
}

renderDevices();
renderSteps();
loadRelease();
loadManifest();
