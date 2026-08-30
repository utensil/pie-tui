use pie_core::word_navigation::{
    SEGMENTATION_UNICODE_VERSION, WordNavOptions, default_word_segments, find_word_backward,
    find_word_forward,
};

fn main() {
    assert_eq!(SEGMENTATION_UNICODE_VERSION, (16, 0, 0));

    let text = "AกB";
    let segments = default_word_segments(text);
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, text);
    assert!(segments[0].is_word_like);
    assert_eq!(find_word_forward(text, 0, &WordNavOptions::default()), 3);
    assert_eq!(find_word_backward(text, 3, &WordNavOptions::default()), 0);

    println!("pie-core fresh consumer: ICU 2.0.0 / Unicode 16.0.0");
}
