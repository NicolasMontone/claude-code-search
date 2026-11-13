#!/bin/sh
# Records demo/out/ccs-launch.mp4 from synthetic data only.
# Needs: ccs on PATH, vhs, ffmpeg, Google Chrome (for the title cards).
set -eu
cd "$(dirname "$0")/.."

mkdir -p demo/out demo/bin
ln -sf ../fake-claude demo/bin/claude
python3 demo/fake_sessions.py demo/out/home

vhs demo/launch.tape

CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
for card in intro outro; do
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
    --window-size=1920,1080 --screenshot="$PWD/demo/out/$card.png" "file://$PWD/demo/cards/$card.html" 2>/dev/null
done

# intro (2.6s) -> recording -> outro (3.4s), with short fades.
ffmpeg -y -loglevel error \
  -loop 1 -t 2.6 -framerate 30 -i demo/out/intro.png \
  -i demo/out/raw.mp4 \
  -loop 1 -t 3.4 -framerate 30 -i demo/out/outro.png \
  -filter_complex "\
[0:v]scale=1920:1080,setsar=1,format=yuv420p,fade=t=in:st=0:d=0.4,fade=t=out:st=2.2:d=0.4[a];\
[1:v]scale=1920:1080,setsar=1,fps=30,format=yuv420p,fade=t=in:st=0:d=0.3[b];\
[2:v]scale=1920:1080,setsar=1,format=yuv420p,fade=t=in:st=0:d=0.4[c];\
[a][b][c]concat=n=3:v=1:a=0[v]" \
  -map "[v]" -c:v libx264 -crf 18 -preset slow -pix_fmt yuv420p -movflags +faststart \
  demo/out/ccs-launch.mp4

echo "wrote demo/out/ccs-launch.mp4"
