//! pie-golden-runner — deterministic render scenario driver.
//!
//! Reads a scenario document (same schema as
//! `crates/pie-term/tests/fixtures/render-golden.json`, produced by
//! tools/golden/gen-golden-render.mjs) on stdin or from a file argument,
//! replays it through the Rust renderer + TestRecorder, and writes the
//! resulting per-frame write buffers as JSON. Running both this and the TS
//! generator over the same scenarios must produce identical documents — the
//! differential oracle behind the M2 goldens.

use std::io::Read;

use pie_app::{LineSource, MainScreenController};
use pie_term::TestRecorder;

struct ScriptedLines(Vec<String>);
impl LineSource for ScriptedLines {
    fn render_lines(&mut self, _width: usize) -> Vec<String> {
        self.0.clone()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    if let Some(path) = std::env::args().nth(1) {
        input = std::fs::read_to_string(&path)?;
    } else {
        std::io::stdin().read_to_string(&mut input)?;
    }
    let root: serde_json::Value = serde_json::from_str(&input)?;

    let mut out_scenarios = Vec::new();
    for sc in root["scenarios"].as_array().ok_or("missing scenarios")? {
        let width = sc["width"].as_u64().ok_or("missing width")? as usize;
        let height = sc["height"].as_u64().ok_or("missing height")? as usize;
        let name = sc["name"].as_str().ok_or("missing name")?.to_string();

        let mut term = TestRecorder::new(width, height);
        let mut renderer = MainScreenController::new();
        let mut frame_writes: Vec<serde_json::Value> = Vec::new();
        let mut output_frames: Vec<serde_json::Value> = Vec::new();

        for (frame_idx, frame) in sc["frames"]
            .as_array()
            .ok_or("missing frames")?
            .iter()
            .enumerate()
        {
            let frame_width = frame["width"].as_u64().ok_or("frame missing width")? as usize;
            let frame_height = frame["height"].as_u64().ok_or("frame missing height")? as usize;
            let lines: Vec<String> = frame["lines"]
                .as_array()
                .ok_or("frame lines must be an array")?
                .iter()
                .map(|l| l.as_str().map(str::to_string).unwrap_or_default())
                .collect();
            term.set_size(frame_width, frame_height);
            term.clear_written();
            let mut source = ScriptedLines(lines.clone());
            renderer
                .render_now(&mut term, &mut source, frame_idx == 0)
                .map_err(|e| format!("render error in {name} frame {frame_idx}: {e:?}"))?;
            frame_writes.push(serde_json::Value::Array(
                term.write_boundaries
                    .iter()
                    .enumerate()
                    .map(|(i, start)| {
                        let end = term
                            .write_boundaries
                            .get(i + 1)
                            .copied()
                            .unwrap_or(term.written.len());
                        serde_json::Value::String(term.written[*start..end].to_string())
                    })
                    .collect(),
            ));
            output_frames.push(serde_json::json!({
                "width": frame_width,
                "height": frame_height,
                "lines": lines,
            }));
        }

        out_scenarios.push(serde_json::json!({
            "name": name,
            "width": width,
            "height": height,
            "frames": output_frames,
            "frameWrites": frame_writes,
        }));
    }

    let doc = serde_json::json!({ "scenarios": out_scenarios });
    println!("{doc}");
    Ok(())
}
