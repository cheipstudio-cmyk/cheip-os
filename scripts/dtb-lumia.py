#!/usr/bin/env python3
"""Lumia 735 additions to the upstream device tree, applied to the compiled DTB.

Usage: dtb-lumia.py in.dtb out.dtb

1. All four cores: the Windows Phone UEFI parks CPU1-3 with the parking
   protocol (mailbox 0x301000 + n * 0x1000, from the MSM8x26 Lumia ACPI MADT).
   The MPPark memory stays reserved so Linux never overwrites the parked cores.
2. Wi-Fi: the msm8226 dtsi already describes the Pronto core and the WCN3660
   radio but leaves them disabled. Enable them with the PM8226 supplies used by
   the reference design (same as other MSM8926 phones) and the firmware the
   cheip-firmware service copies from the Windows partition.
"""
import re
import subprocess
import sys

FIRMWARE = "qcom/msm8926/microsoft/superman/wcnss.mbn"


def block(dts, start):
    """End index (after the closing '};') of the node whose '{' is at or after start."""
    i = dts.index("{", start)
    depth = 0
    for j in range(i, len(dts)):
        if dts[j] == "{":
            depth += 1
        elif dts[j] == "}":
            depth -= 1
            if depth == 0:
                return j + 2
    raise ValueError("unbalanced node")


def set_prop(body, name, value):
    """Set or add 'name = value;' right inside the node text body (first level only)."""
    head_end = body.index("{") + 1
    # only look at properties before the first child node
    first_child = re.search(r"\n\t+[\w@,.-]+ \{", body[head_end:])
    limit = head_end + (first_child.start() if first_child else len(body))
    pat = re.compile(r"\n(\t+)" + re.escape(name) + r"( = [^;]*)?;")
    m = pat.search(body, head_end, limit)
    line = f"{name} = {value};" if value is not None else f"{name};"
    if m:
        return body[: m.start()] + f"\n{m.group(1)}{line}" + body[m.end():]
    indent = re.match(r"\n(\t+)", body[head_end:])
    ind = indent.group(1) if indent else "\t"
    return body[:head_end] + f"\n{ind}{line}" + body[head_end:]


def edit_node(dts, header_regex, fn, start=0):
    m = re.compile(header_regex, re.M).search(dts, start)
    if not m:
        raise SystemExit(f"node not found: {header_regex}")
    end = block(dts, m.start())
    return dts[: m.start()] + fn(dts[m.start():end]) + dts[end:], m.start()


def main():
    src, dst = sys.argv[1], sys.argv[2]
    dts = subprocess.run(["dtc", "-q", "-I", "dtb", "-O", "dts", src], check=True, capture_output=True, text=True).stdout

    used = {int(x, 16) for x in re.findall(r"phandle = <0x([0-9a-f]+)>", dts)}
    nxt = max(used or {0}) + 1

    # --- 1. SMP through the parking protocol --------------------------------
    for n in range(4):
        def cpu(body, n=n):
            body = set_prop(body, "enable-method", '"microsoft,parking-protocol"')
            return set_prop(body, "cpu-release-addr", f"<{0x301000 + n * 0x1000:#x}>")
        dts, _ = edit_node(dts, rf"^\t\tcpu@{n} \{{", cpu)
    if "mppark@300000" not in dts:
        dts, _ = edit_node(dts, r"^\treserved-memory \{", lambda b: b[:-3] + "\n\t\tmppark@300000 {\n\t\t\treg = <0x300000 0x40000>;\n\t\t\tno-map;\n\t\t};\n\t};\n")

    # --- 2. Wi-Fi (Pronto + WCN3660) ----------------------------------------
    # Phandles of the PM8226 LDOs in the RPM regulators node
    reg_start = dts.index("qcom,rpm-pm8226-regulators")
    reg_node_start = dts.rfind("regulators {", 0, reg_start)
    reg_end = block(dts, reg_node_start)
    ph = {}
    for ldo in ["l3", "l6", "l10", "l16", "l24"]:
        m = re.compile(rf"^(\t+){ldo} \{{", re.M).search(dts, reg_node_start, reg_end)
        if not m:
            raise SystemExit(f"regulator {ldo} missing")
        end = block(dts, m.start())
        body = dts[m.start():end]
        p = re.search(r"phandle = <0x([0-9a-f]+)>", body)
        if p:
            ph[ldo] = int(p.group(1), 16)
        else:
            ph[ldo] = nxt
            nxt += 1
            body = set_prop(body, "phandle", f"<{ph[ldo]:#x}>")
            dts = dts[: m.start()] + body + dts[end:]
            reg_end += len(body) - (end - m.start())

    def pronto(body):
        body = set_prop(body, "status", '"okay"')
        body = set_prop(body, "firmware-name", f'"{FIRMWARE}"')
        body = set_prop(body, "vddmx-supply", f"<{ph['l3']:#x}>")
        body = set_prop(body, "vddpx-supply", f"<{ph['l6']:#x}>")
        i = body.index("iris {")
        e = block(body, i)
        iris = body[i:e]
        iris = set_prop(iris, "vddxo-supply", f"<{ph['l10']:#x}>")
        iris = set_prop(iris, "vddrfa-supply", f"<{ph['l24']:#x}>")
        iris = set_prop(iris, "vddpa-supply", f"<{ph['l16']:#x}>")
        iris = set_prop(iris, "vdddig-supply", f"<{ph['l24']:#x}>")
        body = body[:i] + iris + body[e:]
        i = body.index("wcnss {")
        e = block(body, i)
        body = body[:i] + set_prop(body[i:e], "status", '"okay"') + body[e:]
        return body

    dts, _ = edit_node(dts, r"^\t\tremoteproc@fb204000 \{", pronto)

    # --- 2b. Charger: the same settings as the other MSM8226 phones upstream --
    # Without them the PMIC keeps what Windows left, including a minimum input
    # voltage that makes it throttle the charger current to almost nothing.
    def charger(body):
        body = set_prop(body, "qcom,minimum-input-voltage", "<4300000>")
        body = set_prop(body, "qcom,fast-charge-current-limit", "<1000000>")
        return set_prop(body, "usb-charge-current-limit", "<1000000>")
    dts, _ = edit_node(dts, r"^\t+charger@1000 \{", charger)

    # --- 3. Real battery voltage (VBAT_SNS) for the battery level -----------
    # Upstream only lists vph_pwr, the system rail: on the charger it reads the
    # charger output instead of the battery, so the level was guesswork.
    if "vbat_sns" not in dts:
        def vadc(body):
            return body[:-3] + "\n\t\t\t\t\tchannel@6 {\n\t\t\t\t\t\treg = <0x06>;\n\t\t\t\t\t\tqcom,pre-scaling = <0x01 0x03>;\n\t\t\t\t\t\tlabel = \"vbat_sns\";\n\t\t\t\t\t};\n\t\t\t\t};\n"
        dts, _ = edit_node(dts, r"^\t+adc@3100 \{", vadc)

    out = subprocess.run(["dtc", "-q", "-I", "dts", "-O", "dtb", "-o", dst, "-"], input=dts, text=True, capture_output=True)
    if out.returncode:
        raise SystemExit(out.stderr)
    print(f"DTB patched: SMP parking protocol, Wi-Fi pronto enabled (supplies {ph})")


if __name__ == "__main__":
    main()
