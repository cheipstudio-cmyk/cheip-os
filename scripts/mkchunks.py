#!/usr/bin/env python3
"""Split a raw disk image into gzip chunks for the web installer.

All-zero 1 MiB blocks are skipped (the installer erases them instead),
runs of data blocks become chunks of up to 16 MiB. Output: manifest.json
plus NNNN.gz files, served next to the site so the browser can fetch them.
"""
import gzip, hashlib, json, os, sys

BLOCK = 1 << 20
CHUNK = 16 << 20

def main(img, out, version):
    os.makedirs(out, exist_ok=True)
    size = os.path.getsize(img)
    chunks, run, run_start, zero = [], bytearray(), 0, bytes(BLOCK)

    def flush():
        nonlocal run
        if not run:
            return
        name = f"{len(chunks):04d}.gz"
        data = gzip.compress(bytes(run), 6, mtime=0)
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)
        chunks.append({"file": name, "offset": run_start, "length": len(run),
                       "gz": len(data), "sha256": hashlib.sha256(run).hexdigest()})
        run = bytearray()

    with open(img, "rb") as f:
        pos = 0
        while pos < size:
            blk = f.read(BLOCK)
            if blk[:len(zero)] == zero[:len(blk)]:
                flush()
            else:
                if not run:
                    run_start = pos
                run += blk
                if len(run) >= CHUNK:
                    flush()
            pos += len(blk)
    flush()
    manifest = {"version": version, "size": size, "block": BLOCK, "chunks": chunks,
                "data": sum(c["length"] for c in chunks), "download": sum(c["gz"] for c in chunks)}
    with open(os.path.join(out, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print(f"{len(chunks)} chunks, {manifest['data'] >> 20} MiB data, {manifest['download'] >> 20} MiB download")

if __name__ == "__main__":
    main(*sys.argv[1:4])
