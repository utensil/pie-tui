//! Differential render goldens — replay the scripted scenarios through the
//! Rust MainScreenRenderer and require byte-equal write buffers vs the pinned
//! reference TuiMainScreen (fixtures harvested by tools/golden/gen-golden-render.mjs).
use pie_core::frame::LogicalFrame;
use pie_term::renderer::{LineSource, MainScreenRenderer};
use pie_term::{Terminal, TestRecorder};

struct FixtureFrame {
    width: usize,
    height: usize,
    lines: Vec<String>,
    writes: Vec<String>,
}

struct FixtureScenario {
    name: String,
    frames: Vec<FixtureFrame>,
}

fn scenarios() -> Vec<FixtureScenario> {
    let raw = include_str!("fixtures/render-golden.json");
    let root: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let mut out = Vec::new();
    for sc in root["scenarios"].as_array().unwrap() {
        let writes_per_frame: Vec<Vec<String>> = sc["frameWrites"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                w.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect()
            })
            .collect();
        let frames: Vec<FixtureFrame> = sc["frames"]
            .as_array()
            .unwrap()
            .iter()
            .zip(writes_per_frame)
            .map(|(frame, writes)| FixtureFrame {
                width: frame["width"].as_u64().unwrap() as usize,
                height: frame["height"].as_u64().unwrap() as usize,
                lines: frame["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|line| line.as_str().unwrap().to_string())
                    .collect(),
                writes,
            })
            .collect();
        out.push(FixtureScenario {
            name: sc["name"].as_str().unwrap().to_string(),
            frames,
        });
    }
    out
}

fn writes(term: &TestRecorder) -> Vec<String> {
    term.write_boundaries
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = term
                .write_boundaries
                .get(index + 1)
                .copied()
                .unwrap_or(term.written.len());
            term.written[*start..end].to_string()
        })
        .collect()
}

#[test]
fn renderer_oracle_is_exactly_pinned_and_non_vacuous() {
    let root: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/render-golden.json")).expect("fixture json");
    assert_eq!(root["generator"], "gen-golden-render.mjs");
    assert_eq!(root["reference"], "0.84.1");
    assert_eq!(root["scenarios"].as_array().unwrap().len(), 11);
    assert_eq!(
        root["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .map(|scenario| scenario["frames"].as_array().unwrap().len())
            .sum::<usize>(),
        28
    );
}

#[test]
fn resize_oracles_change_geometry_and_require_full_clear() {
    let root: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/render-golden.json")).expect("fixture json");
    for name in ["width-change-full", "height-change-full"] {
        let scenario = root["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .find(|scenario| scenario["name"] == name)
            .unwrap();
        let frames = scenario["frames"].as_array().unwrap();
        let first = (
            frames[0]["width"].as_u64().expect("per-frame width"),
            frames[0]["height"].as_u64().expect("per-frame height"),
        );
        let resized = (
            frames[1]["width"].as_u64().expect("per-frame width"),
            frames[1]["height"].as_u64().expect("per-frame height"),
        );
        assert_ne!(first, resized, "{name} must actually change geometry");
        let writes = scenario["frameWrites"][1]
            .as_array()
            .unwrap()
            .iter()
            .map(|write| write.as_str().unwrap())
            .collect::<String>();
        assert!(
            writes.contains("\x1b[2J\x1b[H\x1b[3J"),
            "{name} resize must clear, home, and clear scrollback"
        );
    }

    let above_viewport = root["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scenario| scenario["name"] == "shrink-below-height")
        .unwrap();
    assert!(
        above_viewport["frameWrites"][1]
            .as_array()
            .unwrap()
            .iter()
            .any(|write| write.as_str().unwrap().contains("\x1b[2J\x1b[H\x1b[3J")),
        "a change above the prior viewport must force a full clear"
    );
}

struct ScriptedLines(Vec<String>);
impl LineSource for ScriptedLines {
    fn render_lines(&mut self, _width: usize) -> Vec<String> {
        self.0.clone()
    }
}

#[test]
fn render_buffers_match_reference_byte_for_byte() {
    for sc in scenarios() {
        let first = sc.frames.first().expect("scenario has a frame");
        let mut term = TestRecorder::new(first.width, first.height);
        let mut renderer = MainScreenRenderer::new();
        renderer.set_stopped(false);
        for (frame_idx, frame) in sc.frames.iter().enumerate() {
            let previous_geometry = renderer.capture_render_state();
            let redraws_before = renderer.full_redraws();
            term.set_size(frame.width, frame.height);
            let mut source = ScriptedLines(frame.lines.clone());
            term.clear_written();
            // The generator drives frame 0 with renderNow(true) (force).
            renderer
                .render_now(&mut term, &mut source, frame_idx == 0)
                .unwrap_or_else(|e| {
                    panic!("{} frame {}: render error {:?}", sc.name, frame_idx, e)
                });
            let expected: String = frame.writes.concat();
            assert_eq!(
                term.written, expected,
                "scenario {:?} frame {} byte mismatch",
                sc.name, frame_idx
            );
            assert_eq!(
                writes(&term),
                frame.writes,
                "scenario {:?} frame {} write-boundary mismatch",
                sc.name,
                frame_idx
            );
            let state = renderer.capture_render_state();
            assert_eq!(state.previous_width, frame.width as isize);
            assert_eq!(state.previous_height, frame.height as isize);
            let geometry_changed = frame_idx > 0
                && (previous_geometry.previous_width != frame.width as isize
                    || previous_geometry.previous_height != frame.height as isize);
            if geometry_changed {
                assert_eq!(renderer.full_redraws(), redraws_before + 1);
                assert!(
                    term.written.contains("\x1b[2J\x1b[H\x1b[3J"),
                    "{} frame {} resize did not full-clear",
                    sc.name,
                    frame_idx
                );
            }
            if frame_idx == 2 && sc.name.ends_with("change-full") {
                assert_eq!(
                    renderer.full_redraws(),
                    redraws_before,
                    "{} stable post-resize frame redrew again",
                    sc.name
                );
                assert!(!term.written.contains("\x1b[2J\x1b[H\x1b[3J"));
            }
        }
    }
}

#[test]
fn overflow_line_is_an_error_not_a_crash() {
    // Reference throws when a rendered line exceeds terminal width; the Rust
    // renderer returns RenderError::LineTooWide (documented deviation).
    let mut term = TestRecorder::new(10, 5);
    let mut renderer = MainScreenRenderer::new();
    let mut source = ScriptedLines(vec!["short".to_string()]);
    renderer.render_now(&mut term, &mut source, false).unwrap();
    let state_before = renderer.capture_render_state();
    let redraws_before = renderer.full_redraws();
    term.clear_written();
    let mut source = ScriptedLines(vec![
        "this line is way too wide for ten columns".to_string(),
    ]);
    let err = renderer
        .render_now(&mut term, &mut source, true)
        .unwrap_err();
    assert_eq!(
        err,
        pie_term::renderer::RenderError::LineTooWide {
            index: 0,
            visible: 41,
            width: 10
        }
    );
    assert_eq!(term.written, "", "validation must precede every write");
    assert_eq!(renderer.capture_render_state(), state_before);
    assert_eq!(renderer.full_redraws(), redraws_before);
}

#[test]
fn direct_frame_geometry_mismatch_is_transactional() {
    let mut term = TestRecorder::new(10, 5);
    let mut renderer = MainScreenRenderer::new();
    let before = renderer.capture_render_state();
    let frame = LogicalFrame::new(vec!["ok".into()], 9, 5).unwrap();
    assert_eq!(
        renderer.render_frame(&mut term, &frame, false),
        Err(pie_term::renderer::RenderError::TerminalGeometryChanged {
            frame_width: 9,
            frame_height: 5,
            terminal_width: 10,
            terminal_height: 5,
        })
    );
    assert_eq!(term.written, "");
    assert_eq!(renderer.capture_render_state(), before);
}

#[test]
fn ansi_plan_is_pure_and_preserves_reference_write_boundaries() {
    let scenario = scenarios().remove(0);
    let fixture = &scenario.frames[0];
    let frame = LogicalFrame::new(fixture.lines.clone(), fixture.width, fixture.height).unwrap();
    let mut renderer = MainScreenRenderer::new();
    let before = renderer.capture_render_state();
    let plan = renderer.plan_frame(&frame, true).unwrap();

    assert_eq!(plan.writes(), fixture.writes);
    assert_eq!(renderer.capture_render_state(), before);

    let mut term = TestRecorder::new(fixture.width, fixture.height);
    renderer.render_frame(&mut term, &frame, true).unwrap();
    assert_eq!(writes(&term), fixture.writes);
    assert_ne!(renderer.capture_render_state(), before);
}

#[derive(Debug, PartialEq, Eq)]
enum CursorEvent {
    Write(String),
    Hide,
    Show,
}

struct CursorBoundaryTerminal {
    width: usize,
    height: usize,
    events: Vec<CursorEvent>,
}

impl Terminal for CursorBoundaryTerminal {
    fn start(&mut self, _on_input: pie_term::InputHandler, _on_resize: pie_term::ResizeHandler) {}
    fn stop(&mut self) {}
    fn write(&mut self, data: &str) {
        self.events.push(CursorEvent::Write(data.into()));
    }
    fn columns(&self) -> usize {
        self.width
    }
    fn rows(&self) -> usize {
        self.height
    }
    fn kitty_protocol_active(&self) -> bool {
        false
    }
    fn move_by(&mut self, _lines: isize) {}
    fn hide_cursor(&mut self) {
        self.events.push(CursorEvent::Hide);
    }
    fn show_cursor(&mut self) {
        self.events.push(CursorEvent::Show);
    }
    fn clear_line(&mut self) {}
    fn clear_from_cursor(&mut self) {}
    fn clear_screen(&mut self) {}
    fn set_title(&mut self, _title: &str) {}
    fn set_progress(&mut self, _active: bool) {}
}

#[test]
fn renderer_replays_cursor_visibility_as_a_terminal_operation() {
    let mut renderer = MainScreenRenderer::new();
    renderer.set_show_hardware_cursor(true);
    let frame =
        LogicalFrame::new(vec![format!("a{}b", pie_core::screen::CURSOR_MARKER)], 4, 2).unwrap();
    let mut terminal = CursorBoundaryTerminal {
        width: 4,
        height: 2,
        events: Vec::new(),
    };
    renderer.render_frame(&mut terminal, &frame, false).unwrap();
    assert!(matches!(terminal.events.last(), Some(CursorEvent::Show)));
    assert!(
        terminal
            .events
            .iter()
            .any(|event| matches!(event, CursorEvent::Write(write) if write == "\x1b[2G"))
    );
    assert!(
        terminal
            .events
            .iter()
            .all(|event| !matches!(event, CursorEvent::Write(write) if write == "\x1b[?25h"))
    );
}

struct CountingLines {
    calls: usize,
}

impl LineSource for CountingLines {
    fn render_lines(&mut self, _width: usize) -> Vec<String> {
        self.calls += 1;
        vec!["must not render".into()]
    }
}

#[test]
fn stopped_legacy_force_resets_state_without_rendering_or_writing() {
    let mut term = TestRecorder::new(40, 8);
    let mut renderer = MainScreenRenderer::new();
    let mut initial = ScriptedLines(vec!["hello".to_string()]);
    renderer.render_now(&mut term, &mut initial, false).unwrap();
    let state_before = renderer.capture_render_state();
    let redraws_before = renderer.full_redraws();
    assert_eq!(state_before.previous_width, 40);
    assert_eq!(state_before.previous_height, 8);
    assert!(!state_before.previous_lines.is_empty());
    term.clear_written();

    renderer.set_stopped(true);
    let mut source = CountingLines { calls: 0 };
    renderer.render_now(&mut term, &mut source, true).unwrap();
    assert_eq!(term.written, "");
    assert_eq!(source.calls, 0);
    assert_eq!(
        renderer.capture_render_state(),
        pie_term::renderer::RenderState {
            previous_lines: Vec::new(),
            previous_width: -1,
            previous_height: -1,
            cursor_row: 0,
            hardware_cursor_row: 0,
            max_lines_rendered: 0,
            previous_viewport_top: 0,
        }
    );
    assert_eq!(renderer.full_redraws(), redraws_before);
    assert!(!term.started());
    let _ = std::io::Write::flush(&mut std::io::sink()); // keep `Terminal` trait in scope
    let _t: &mut dyn Terminal = &mut term;
}

#[test]
fn stopped_frame_and_plan_apis_are_strict_no_ops_even_when_forced() {
    let mut term = TestRecorder::new(40, 8);
    let mut renderer = MainScreenRenderer::new();
    let mut initial = ScriptedLines(vec!["initial".into()]);
    renderer.render_now(&mut term, &mut initial, false).unwrap();
    renderer.set_stopped(true);
    let state_before = renderer.capture_render_state();
    let redraws_before = renderer.full_redraws();
    term.clear_written();

    let frame = LogicalFrame::new(vec!["changed".into()], 40, 8).unwrap();
    renderer.render_frame(&mut term, &frame, true).unwrap();
    assert_eq!(term.written, "");
    assert_eq!(renderer.capture_render_state(), state_before);
    assert_eq!(renderer.full_redraws(), redraws_before);

    let plan = renderer.plan_frame(&frame, true).unwrap();
    assert!(plan.is_empty());
    assert_eq!(renderer.capture_render_state(), state_before);
    assert_eq!(renderer.full_redraws(), redraws_before);
}

#[test]
fn kitty_blocks_reserve_rows_and_delete_only_owned_changed_images() {
    let image = "\x1b_Ga=T,f=100,q=2,c=2,r=2,i=7;QUJDRA==\x1b\\";
    let mut term = TestRecorder::new(8, 4);
    let mut renderer = MainScreenRenderer::new();
    let initial = LogicalFrame::new(
        vec![image.to_owned(), String::new(), "tail".to_owned()],
        8,
        4,
    )
    .unwrap();
    renderer.render_frame(&mut term, &initial, false).unwrap();
    assert_eq!(
        writes(&term)[0],
        format!("\x1b[?2026h\r\n\x1b[1A{image}\x1b[1B\r\ntail\x1b[0m\x1b]8;;\x07\x1b[?2026l")
    );

    term.clear_written();
    let removed = LogicalFrame::new(vec!["gone".to_owned(), "tail".to_owned()], 8, 4).unwrap();
    renderer.render_frame(&mut term, &removed, false).unwrap();
    assert_eq!(
        writes(&term)[0],
        "\x1b[?2026h\x1b_Ga=d,d=I,i=7,q=2\x1b\\\x1b[2A\r\x1b[2Kgone\x1b[0m\x1b]8;;\x07\r\n\x1b[2Ktail\x1b[0m\x1b]8;;\x07\r\n\x1b[2K\x1b[1A\x1b[?2026l"
    );
}

#[test]
fn restoring_render_state_drops_foreign_kitty_ownership() {
    let image = "\x1b_Ga=T,f=100,q=2,c=1,r=1,i=9;QUFBQQ==\x1b\\";
    let mut source = MainScreenRenderer::new();
    let frame = LogicalFrame::new(vec![image.to_owned()], 8, 3).unwrap();
    source
        .render_frame(&mut TestRecorder::new(8, 3), &frame, false)
        .unwrap();

    let mut restored = MainScreenRenderer::new();
    restored.restore_render_state(source.capture_render_state());
    assert_eq!(restored.capture_render_state().previous_lines, vec![""]);
    let replacement = LogicalFrame::new(vec!["plain".to_owned()], 9, 3).unwrap();
    let plan = restored.plan_frame(&replacement, false).unwrap();
    assert!(
        plan.writes()
            .iter()
            .all(|write| !write.contains("a=d,d=I,i=9"))
    );
}
