#!/bin/sh
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-16
#
# Regenerates the synthetic replay clips in direlay-linux/tests/data: 3 s of moving testsrc2 at
# 1280x720 and 30 fps with a single IDR, as H.264 High L3.2 and HEVC Main L4.0, plus their
# oxp-annexb-index/1 timing indexes. Needs ffmpeg with libx264 and libx265 (FFMPEG overrides it).
# SEI and access unit delimiters are removed, like the iPhone's stream.
set -eu
cd "$(dirname "$0")/.."
FFMPEG=${FFMPEG:-ffmpeg}
OUT=direlay-linux/tests/data
SOURCE=testsrc2=size=1280x720:rate=30:duration=3
mkdir -p "$OUT"

"$FFMPEG" -hide_banner -loglevel error -y -f lavfi -i "$SOURCE" -pix_fmt yuv420p \
  -c:v libx264 -profile:v high -level:v 3.2 -preset medium -tune zerolatency \
  -x264-params keyint=infinite:min-keyint=infinite:scenecut=0:bframes=0 \
  -b:v 350k -maxrate 500k -bufsize 1000k \
  -bsf:v 'filter_units=remove_types=6|9' -f h264 "$OUT/replay-h264-high.h264"

"$FFMPEG" -hide_banner -loglevel error -y -f lavfi -i "$SOURCE" -pix_fmt yuv420p \
  -c:v libx265 -profile:v main -preset medium -tune zerolatency \
  -x265-params keyint=-1:min-keyint=-1:scenecut=0:bframes=0:info=0:level-idc=40:log-level=error \
  -b:v 350k -maxrate 500k -bufsize 1000k \
  -bsf:v 'filter_units=remove_types=35|39|40' -f hevc "$OUT/replay-h265-main.h265"

cargo run --offline --quiet -p openxplay-relay --example replay_index -- \
  "$OUT/replay-h264-high.h264" "$OUT/replay-h265-main.h265"
