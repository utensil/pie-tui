use pie_app::{LineSource, MainScreenController};
use pie_term::TestRecorder;

struct ScriptedLines {
    lines: Vec<String>,
    calls: usize,
    widths: Vec<usize>,
}

impl LineSource for ScriptedLines {
    fn render_lines(&mut self, width: usize) -> Vec<String> {
        self.calls += 1;
        self.widths.push(width);
        self.lines.clone()
    }
}

fn writes(terminal: &TestRecorder) -> Vec<String> {
    terminal
        .write_boundaries
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = terminal
                .write_boundaries
                .get(index + 1)
                .copied()
                .unwrap_or(terminal.written.len());
            terminal.written[*start..end].to_string()
        })
        .collect()
}

#[test]
fn controller_path_matches_all_pinned_main_screen_writes() {
    let root: serde_json::Value = serde_json::from_str(include_str!(
        "../../pie-term/tests/fixtures/render-golden.json"
    ))
    .unwrap();
    assert_eq!(root["reference"], "0.84.1");

    let mut frame_count = 0usize;
    for scenario in root["scenarios"].as_array().unwrap() {
        let frames = scenario["frames"].as_array().unwrap();
        let first = &frames[0];
        let mut terminal = TestRecorder::new(
            first["width"].as_u64().unwrap() as usize,
            first["height"].as_u64().unwrap() as usize,
        );
        let mut controller = MainScreenController::new();

        for (index, frame) in frames.iter().enumerate() {
            frame_count += 1;
            let width = frame["width"].as_u64().unwrap() as usize;
            let height = frame["height"].as_u64().unwrap() as usize;
            terminal.set_size(width, height);
            terminal.clear_written();
            let mut source = ScriptedLines {
                lines: frame["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|line| line.as_str().unwrap().to_string())
                    .collect(),
                calls: 0,
                widths: Vec::new(),
            };

            controller
                .render_now(&mut terminal, &mut source, index == 0)
                .unwrap();
            let expected: Vec<String> = scenario["frameWrites"][index]
                .as_array()
                .unwrap()
                .iter()
                .map(|write| write.as_str().unwrap().to_string())
                .collect();
            assert_eq!(
                writes(&terminal),
                expected,
                "{} frame {index}",
                scenario["name"].as_str().unwrap()
            );
            assert_eq!(source.calls, 1);
            assert_eq!(source.widths, vec![width]);
        }
    }
    assert_eq!(frame_count, 28);
}

#[test]
fn stopped_controller_does_not_render_or_mutate_state() {
    let mut terminal = TestRecorder::new(20, 5);
    let mut controller = MainScreenController::new();
    let mut initial = ScriptedLines {
        lines: vec!["initial".into()],
        calls: 0,
        widths: Vec::new(),
    };
    controller
        .render_now(&mut terminal, &mut initial, false)
        .unwrap();
    let state = controller.capture_render_state();
    let redraws = controller.full_redraws();
    terminal.clear_written();

    controller.set_stopped(true);
    let mut source = ScriptedLines {
        lines: vec!["changed".into()],
        calls: 0,
        widths: Vec::new(),
    };
    controller
        .render_now(&mut terminal, &mut source, true)
        .unwrap();

    assert_eq!(source.calls, 0);
    assert_eq!(terminal.written, "");
    assert_eq!(controller.capture_render_state(), state);
    assert_eq!(controller.full_redraws(), redraws);
}
