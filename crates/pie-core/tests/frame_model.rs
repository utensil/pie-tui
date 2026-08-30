use pie_core::frame::{FrameDiff, LogicalFrame, LogicalFrameError};
use pie_core::screen::{CURSOR_MARKER, CursorPos, SEGMENT_RESET};

#[test]
fn logical_frame_extracts_cursor_before_appending_resets() {
    let frame = LogicalFrame::new(
        vec![
            "header".to_string(),
            format!("\x1b[31mprompt> {CURSOR_MARKER}value\x1b[0m"),
        ],
        40,
        8,
    )
    .unwrap();

    assert_eq!(frame.cursor(), Some(CursorPos { row: 1, col: 8 }));
    assert_eq!(frame.width(), 40);
    assert_eq!(frame.height(), 8);
    assert_eq!(frame.lines().len(), 2);
    assert!(
        frame
            .lines()
            .iter()
            .all(|line| line.ends_with(SEGMENT_RESET))
    );
    assert!(
        frame
            .lines()
            .iter()
            .all(|line| !line.contains(CURSOR_MARKER))
    );
}

#[test]
fn logical_frame_resets_non_images_but_preserves_image_rows() {
    let kitty = "\x1b_Gi=7,r=4;payload\x1b\\".to_string();
    let iterm = "prefix\x1b]1337;File=name=x;width=99:payload\x07".to_string();
    let frame = LogicalFrame::new(
        vec!["plain".to_string(), kitty.clone(), iterm.clone()],
        5,
        3,
    )
    .unwrap();

    assert_eq!(frame.lines()[0], format!("plain{SEGMENT_RESET}"));
    assert_eq!(frame.lines()[1], kitty);
    assert_eq!(frame.lines()[2], iterm);
}

#[test]
fn logical_frame_validates_every_non_image_line() {
    let error = LogicalFrame::new(vec!["ok".to_string(), "this is too wide".to_string()], 8, 4)
        .unwrap_err();

    assert_eq!(
        error,
        LogicalFrameError::LineTooWide {
            index: 1,
            visible: 16,
            width: 8,
        }
    );
}

#[test]
fn frame_diff_classifies_unchanged_append_middle_and_delete() {
    assert_eq!(
        FrameDiff::between(&["a".into(), "b".into()], &["a".into(), "b".into()]),
        FrameDiff {
            first_changed: None,
            last_changed: None,
            appended: false,
            append_start: false,
            deleted_only: false,
        }
    );
    assert_eq!(
        FrameDiff::between(
            &["a".into(), "b".into()],
            &["a".into(), "b".into(), "c".into(), "d".into()],
        ),
        FrameDiff {
            first_changed: Some(2),
            last_changed: Some(3),
            appended: true,
            append_start: true,
            deleted_only: false,
        }
    );
    assert_eq!(
        FrameDiff::between(
            &["a".into(), "b".into(), "c".into()],
            &["a".into(), "x".into(), "c".into()],
        ),
        FrameDiff {
            first_changed: Some(1),
            last_changed: Some(1),
            appended: false,
            append_start: false,
            deleted_only: false,
        }
    );
    assert_eq!(
        FrameDiff::between(&["a".into(), "b".into(), "c".into()], &["a".into()],),
        FrameDiff {
            first_changed: Some(1),
            last_changed: Some(2),
            appended: false,
            append_start: false,
            deleted_only: true,
        }
    );
}

#[test]
fn append_after_an_earlier_change_is_not_the_fast_append_path() {
    let diff = FrameDiff::between(
        &["a".into(), "b".into()],
        &["changed".into(), "b".into(), "c".into()],
    );

    assert_eq!(diff.first_changed, Some(0));
    assert_eq!(diff.last_changed, Some(2));
    assert!(diff.appended);
    assert!(!diff.append_start);
    assert!(!diff.deleted_only);

    let initial_content = FrameDiff::between(&[], &["first".into()]);
    assert!(initial_content.appended);
    assert_eq!(initial_content.first_changed, Some(0));
    assert!(!initial_content.append_start);
}
