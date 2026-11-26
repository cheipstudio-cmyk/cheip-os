// Cheip OS installer: dispositivi, avanzamento passi, link alle release, console USB (fastboot)
import { FastbootDevice, setDebugLevel } from "./vendor/fastboot.min.mjs";

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
      if (a.id === "img-btn") $("#img-info").textContent = `${rel.name || rel.tag_name} · ${human(asset.size)}`;
    } else {
      a.setAttribute("disabled", "");
      a.href = "#";
      if (a.id === "img-btn") $("#img-info").textContent = "The first public build is on its way.";
    }
  });
}

// ---------- Console USB ----------
const out = $("#usb-out");
const log = (t) => { out.textContent += t + "\n"; out.scrollTop = out.scrollHeight; };
let dev = null;

function setConnected(ok, label) {
  $("#usb-dot").classList.toggle("on", ok);
  $("#usb-state").textContent = label;
  $("#usb-boot").disabled = !ok;
  $("#usb-info").disabled = !ok;
}

$("#usb-connect").addEventListener("click", async () => {
  if (!("usb" in navigator)) {
    log("This browser has no WebUSB. Open the page in Chrome or Edge on a PC.");
    return;
  }
  try {
    setDebugLevel(0);
    dev = new FastbootDevice();
    await dev.connect();
    const product = await dev.getVariable("product").catch(() => "");
    setConnected(true, `Connected${product ? " · " + product : ""}`);
    log("Bootloader found. You can boot Cheip OS from RAM.");
  } catch (e) {
    setConnected(false, "Not connected");
    log("Connection failed: " + (e.message || e) + "\nOn Windows the phone needs the Android bootloader (fastboot) driver.");
  }
});

$("#usb-info").addEventListener("click", async () => {
  for (const v of ["product", "version-bootloader", "serialno", "secure", "unlocked"]) {
    const val = await dev.getVariable(v).catch(() => "");
    log(`${v}: ${val || "?"}`);
  }
});

$("#usb-boot").addEventListener("click", () => $("#boot-file").click());

$("#boot-file").addEventListener("change", async (ev) => {
  const file = ev.target.files[0];
  if (!file || !dev) return;
  const bar = $("#usb-bar");
  bar.style.display = "block";
  log(`Sending ${file.name} (${human(file.size)})…`);
  try {
    await dev.bootBlob(file, (p) => { bar.firstElementChild.style.width = Math.round(p * 100) + "%"; });
    log("Started. Cheip OS is booting: the phone disconnects now.");
    setConnected(false, "Booting Cheip OS");
  } catch (e) {
    log("Boot failed: " + (e.message || e));
  } finally {
    ev.target.value = "";
  }
});

renderDevices();
renderSteps();
loadRelease();
