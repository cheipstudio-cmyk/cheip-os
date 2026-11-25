#!/usr/bin/env python3
"""Genera i file finti della demo web: miniature, pagine PDF, media e il
manifest Rust incluso nel wasm (src/vfs_data.rs). Parte dalla home demo
creata da cheip-screens in /tmp/cheip-demo-home."""
import os, subprocess, json, sys
from PIL import Image

SRC = sys.argv[1] if len(sys.argv) > 1 else "/tmp/cheip-demo-home"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "shell/web/demo")
os.makedirs(OUT, exist_ok=True)
for f in os.listdir(OUT):
    os.remove(os.path.join(OUT, f))

def run(*a):
    return subprocess.run(a, capture_output=True, text=True)

def probe(p):
    r = run("ffprobe", "-v", "error", "-show_entries", "format=duration:format_tags=title,artist", "-of", "json", p)
    j = json.loads(r.stdout or "{}").get("format", {})
    tags = {k.lower(): v for k, v in j.get("tags", {}).items()}
    return int(float(j.get("duration", 0))), tags.get("title", ""), tags.get("artist", "")

entries = []
n = 0
def asset(name, data_path):
    global n
    n += 1
    dst = f"a{n:03d}_{name}"
    os.replace(data_path, os.path.join(OUT, dst)) if data_path.startswith("/tmp/gen_") else __import__("shutil").copy(data_path, os.path.join(OUT, dst))
    return dst

ages = {}
age = 600
for dirpath, dirs, files in os.walk(SRC):
    dirs[:] = sorted(d for d in dirs if not d.startswith("."))
    for f in sorted(files):
        if f.startswith("."):
            continue
        p = os.path.join(dirpath, f)
        rel = os.path.relpath(p, SRC)
        ext = f.rsplit(".", 1)[-1].lower()
        e = {"path": "/home/cheip/" + rel, "size": os.path.getsize(p), "age": age, "data": None, "thumb": None,
             "info": "", "title": "", "artist": "", "secs": 0, "pages": []}
        age += 1800
        if ext in ("jpg", "png"):
            img = Image.open(p).convert("RGB")
            img.thumbnail((720, 720)); img.save("/tmp/gen_full.jpg", quality=85)
            e["data"] = asset("full.jpg", "/tmp/gen_full.jpg")
            t = Image.open(p).convert("RGB"); w, h = t.size; s = min(w, h)
            t = t.crop(((w - s) // 2, (h - s) // 2, (w + s) // 2, (h + s) // 2)).resize((240, 240)); t.save("/tmp/gen_t.png")
            e["thumb"] = asset("thumb.png", "/tmp/gen_t.png")
        elif ext == "mp4":
            secs, _, _ = probe(p)
            run("ffmpeg", "-y", "-loglevel", "error", "-ss", "1", "-i", p, "-frames:v", "1", "-vf", "scale=320:-1", "/tmp/gen_v.png")
            e["thumb"] = asset("vthumb.png", "/tmp/gen_v.png")
            run("ffmpeg", "-y", "-loglevel", "error", "-ss", "1", "-i", p, "-frames:v", "1", "/tmp/gen_vf.png")
            e["data"] = asset("vframe.png", "/tmp/gen_vf.png")
            e["secs"] = secs; e["info"] = f"{secs // 60}:{secs % 60:02d}"
        elif ext == "flac":
            secs, title, artist = probe(p)
            r = run("ffmpeg", "-y", "-loglevel", "error", "-i", p, "-an", "-frames:v", "1", "/tmp/gen_c.png")
            if os.path.exists("/tmp/gen_c.png") and r.returncode == 0:
                Image.open("/tmp/gen_c.png").convert("RGB").resize((240, 240)).save("/tmp/gen_c2.png")
                os.remove("/tmp/gen_c.png")
                e["thumb"] = asset("cover.png", "/tmp/gen_c2.png")
            e["secs"] = secs; e["title"] = title; e["artist"] = artist
            e["info"] = f"{artist or 'Unknown artist'} · {secs // 60}:{secs % 60:02d}"
        elif ext == "pdf":
            pages = int([l for l in run("mutool", "info", p).stdout.splitlines() if "Pages:" in l][0].split(":")[1])
            for i in range(1, pages + 1):
                run("mutool", "draw", "-q", "-w", "720", "-o", "/tmp/gen_p.png", p, str(i))
                e["pages"].append(asset(f"page{i}.png", "/tmp/gen_p.png"))
        elif ext in ("docx", "txt"):
            e["data"] = asset(f"doc.{ext}", p)
        entries.append(e)

def lit(s):
    return json.dumps(s, ensure_ascii=False)

def inc(a):
    return f'Some(include_bytes!("../web/demo/{a}"))' if a else "None"

rs = ["// Generato da tools/gen_web_demo.py: file finti della demo web", "use crate::vfs::VEntry;", "", "pub static FILES: &[VEntry] = &["]
for e in entries:
    pages = ", ".join(f'include_bytes!("../web/demo/{a}")' for a in e["pages"])
    rs.append(f'    VEntry {{ path: {lit(e["path"])}, size: {e["size"]}, age: {e["age"]}, data: {inc(e["data"])}, thumb: {inc(e["thumb"])}, info: {lit(e["info"])}, title: {lit(e["title"])}, artist: {lit(e["artist"])}, secs: {e["secs"]}, pages: &[{pages}] }},')
rs.append("];")
open(os.path.join(ROOT, "shell/src/vfs_data.rs"), "w").write("\n".join(rs) + "\n")
total = sum(os.path.getsize(os.path.join(OUT, f)) for f in os.listdir(OUT))
print(f"{len(entries)} file, {len(os.listdir(OUT))} asset, {total // 1024} KB")
