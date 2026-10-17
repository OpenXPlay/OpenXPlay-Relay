// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Writes `<clip>.jsonl`, the `oxp-annexb-index/1` timing index `--record` produces, for raw
//! Annex B clips at a nominal 30 fps: `cargo run --example replay_index -- CLIP.h264 ...`.
use openxplay_relay::replay_input::clip::Clip;

fn main() -> std::io::Result<()> {
    let clips = std::env::args().skip(1).collect::<Vec<_>>();
    if clips.is_empty() {
        eprintln!("Usage: replay_index CLIP.h264|CLIP.h265 ...");
        std::process::exit(2);
    }
    for path in clips {
        let clip = Clip::open_raw(std::path::Path::new(&path))?;
        let mut index = Vec::new();
        clip.write_index(&mut index)?;
        std::fs::write(format!("{path}.jsonl"), index)?;
        println!("{path}: {} frames", clip.frames);
    }
    Ok(())
}
