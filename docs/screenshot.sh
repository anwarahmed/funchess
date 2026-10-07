#!/bin/sh
# Makes docs/screenshot.png, the picture in the README: plays a few moves in the built
# program inside tmux (on a private socket), captures the screen with its colors, and
# draws that as an image. Needs tmux, python3, rsvg-convert and a monospace font.
#
#   cargo build --release && docs/screenshot.sh
set -eu
cd "$(dirname "$0")/.."
BIN=$PWD/target/release/funchess
[ -x "$BIN" ] || { echo "build first: cargo build --release" >&2; exit 1; }
TMP=$(mktemp -d)
T() { tmux -L funchess-shot "$@"; }
trap 'T kill-server 2>/dev/null || true; rm -rf "$TMP"' EXIT

T new-session -d -s shot -x 132 -y 50 "env COLORTERM=truecolor XDG_STATE_HOME='$TMP/xdg' FUNCHESS_NO_UPDATE=1 '$BIN' local; sleep 30"
sleep 1
# The Italian game, then the knight on f3 picked up to show where it can go.
T send-keys -t shot -l e2e4e7e5g1f3b8c6f1c4f8c5f3
sleep 1
T capture-pane -p -e -t shot > "$TMP/screen.txt"

python3 - "$TMP/screen.txt" "$TMP/screen.svg" <<'PY'
import sys, re, html
# A tmux `capture-pane -p -e` dump as an SVG picture: one rectangle per run of cells,
# half blocks as half-height rectangles, and every other character placed on the grid.
CW, CH, PAD, FS = 9, 18, 18, 15
DFG, DBG = (205,208,214), (22,22,30)
def c256(n):
    base=[(40,42,54),(224,90,90),(120,200,120),(220,200,110),(100,150,240),(200,120,220),(90,200,215),(205,208,214),(110,114,130),(255,110,110),(140,230,140),(240,225,130),(130,170,255),(225,150,240),(110,225,235),(255,255,255)]
    if n<16: return base[n]
    if n<232:
        n-=16; l=[0,95,135,175,215,255]; return (l[n//36],l[n//6%6],l[n%6])
    v=8+10*(n-232); return (v,v,v)
rows=[]
for line in open(sys.argv[1],encoding='utf-8').read().rstrip('\n').split('\n'):
    fg,bg,bold=DFG,DBG,False; cells=[]; i=0
    while i<len(line):
        m=re.match(r'\x1b\[([0-9;]*)m',line[i:])
        if m:
            p=[int(x) if x else 0 for x in m.group(1).split(';')]; j=0
            while j<len(p):
                a=p[j]
                if a==0: fg,bg,bold=DFG,DBG,False
                elif a==1: bold=True
                elif a==22: bold=False
                elif a in(38,48):
                    if p[j+1]==2: c=tuple(p[j+2:j+5]); j+=4
                    else: c=c256(p[j+2]); j+=2
                    if a==38: fg=c
                    else: bg=c
                elif a==39: fg=DFG
                elif a==49: bg=DBG
                elif 30<=a<=37: fg=c256(a-30)
                elif 90<=a<=97: fg=c256(a-82)
                j+=1
            i+=m.end(); continue
        cells.append((line[i],fg,bg,bold)); i+=1
    rows.append(cells)
cols=max(len(r) for r in rows)
W,H=cols*CW+2*PAD, len(rows)*CH+2*PAD
hexc=lambda c:'#%02x%02x%02x'%c
out=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
     f'<rect width="{W}" height="{H}" rx="10" fill="{hexc(DBG)}"/>','<g shape-rendering="crispEdges">']
def runs(row, key):
    start=0
    for x in range(1,len(row)+1):
        if x==len(row) or key(row[x])!=key(row[start]):
            yield start,x,key(row[start]); start=x
for y,row in enumerate(rows):
    py=PAD+y*CH
    for a,b,c in runs(row, lambda cell: cell[2]):
        if c!=DBG: out.append(f'<rect x="{PAD+a*CW}" y="{py}" width="{(b-a)*CW}" height="{CH}" fill="{hexc(c)}"/>')
    for a,b,c in runs(row, lambda cell: cell[1] if cell[0]=='▀' else None):
        if c: out.append(f'<rect x="{PAD+a*CW}" y="{py}" width="{(b-a)*CW}" height="{CH//2}" fill="{hexc(c)}"/>')
out.append('</g>')
out.append(f'<g font-family="JetBrainsMono Nerd Font, JetBrains Mono, ui-monospace, Menlo, Consolas, monospace" font-size="{FS}">')
for y,row in enumerate(rows):
    for a,b,k in runs(row, lambda cell: (cell[1],cell[3]) if cell[0] not in ' ▀' else None):
        if k:
            xs=' '.join(str(PAD+x*CW) for x in range(a,b)); t=html.escape(''.join(c[0] for c in row[a:b]))
            out.append(f'<text x="{xs}" y="{PAD+y*CH+CH-5}" fill="{hexc(k[0])}"{" font-weight=\"bold\"" if k[1] else ""} xml:space="preserve">{t}</text>')
out.append('</g></svg>')
open(sys.argv[2],'w').write('\n'.join(out))
PY
rsvg-convert "$TMP/screen.svg" -o docs/screenshot.png
echo "wrote docs/screenshot.png"
