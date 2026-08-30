//! Differential contract for the pinned pi-tui `Image` component.
//!
//! The JS oracle retains and returns one Array object for a same-width cache
//! hit. `Component::render` returns an owned Rust `Vec`, so pointer identity is
//! not claimed here. `ImageCacheStats::allocation_generation` instead proves
//! that the component retains one internal cached allocation until width
//! changes or `invalidate()` drops it.

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, MutexGuard};

use pie_components::{
    Component, Image, ImageCacheStats, ImageEnvironment, ImageOptions, ImageTheme,
    KittyImageDeletionOwner,
};
use pie_core::terminal_image::{CellDimensions, ImageDimensions, ImageProtocol};
use pie_term::capabilities::TerminalCapabilities;

static FIXTURE: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();

fn fixture() -> &'static serde_json::Value {
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!("fixtures/image-component.json"))
            .expect("image-component.json is valid JSON")
    })
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug)]
struct Facts {
    capabilities: TerminalCapabilities,
    cells: CellDimensions,
    home: String,
    ids: VecDeque<NonZeroU32>,
    allocation_calls: usize,
}

#[derive(Clone)]
struct FactsHandle(Arc<Mutex<Facts>>);

impl FactsHandle {
    fn set(&self, protocol: Option<ImageProtocol>, cells: CellDimensions, hyperlinks: bool) {
        let mut facts = lock(&self.0);
        facts.capabilities = capabilities(protocol, hyperlinks);
        facts.cells = cells;
    }

    fn allocation_calls(&self) -> usize {
        lock(&self.0).allocation_calls
    }
}

struct TestEnvironment(FactsHandle);

impl ImageEnvironment for TestEnvironment {
    fn capabilities(&self) -> TerminalCapabilities {
        lock(&self.0.0).capabilities
    }

    fn cell_dimensions(&self) -> CellDimensions {
        lock(&self.0.0).cells
    }

    fn allocate_image_id(&mut self) -> NonZeroU32 {
        let mut facts = lock(&self.0.0);
        facts.allocation_calls += 1;
        facts.ids.pop_front().expect("test image ID is injected")
    }

    fn home_dir(&self) -> String {
        lock(&self.0.0).home.clone()
    }
}

fn environment(
    protocol: Option<ImageProtocol>,
    cells: CellDimensions,
    hyperlinks: bool,
    ids: impl IntoIterator<Item = u32>,
) -> (Box<dyn ImageEnvironment>, FactsHandle) {
    let handle = FactsHandle(Arc::new(Mutex::new(Facts {
        capabilities: capabilities(protocol, hyperlinks),
        cells,
        home: "/opt/pi-home".to_owned(),
        ids: ids
            .into_iter()
            .map(|id| NonZeroU32::new(id).expect("test IDs are nonzero"))
            .collect(),
        allocation_calls: 0,
    })));
    (Box::new(TestEnvironment(handle.clone())), handle)
}

fn capabilities(images: Option<ImageProtocol>, hyperlinks: bool) -> TerminalCapabilities {
    TerminalCapabilities {
        images,
        true_color: true,
        hyperlinks,
    }
}

fn cells(width_px: f64, height_px: f64) -> CellDimensions {
    CellDimensions {
        width_px,
        height_px,
    }
}

fn dimensions(width_px: u32, height_px: u32) -> ImageDimensions {
    ImageDimensions {
        width_px,
        height_px,
    }
}

fn fixture_dimensions(value: &serde_json::Value) -> ImageDimensions {
    dimensions(
        value["widthPx"].as_u64().expect("widthPx") as u32,
        value["heightPx"].as_u64().expect("heightPx") as u32,
    )
}

fn lines(value: &serde_json::Value) -> Vec<String> {
    serde_json::from_value(value.clone()).expect("fixture lines")
}

fn theme() -> ImageTheme {
    ImageTheme::new(|value| format!("\x1b[35m{value}\x1b[0m"))
}

struct EnvironmentSpec {
    protocol: Option<ImageProtocol>,
    cell_dimensions: CellDimensions,
    hyperlinks: bool,
    ids: Vec<u32>,
}

fn environment_spec(
    protocol: Option<ImageProtocol>,
    cell_dimensions: CellDimensions,
    hyperlinks: bool,
    ids: impl IntoIterator<Item = u32>,
) -> EnvironmentSpec {
    EnvironmentSpec {
        protocol,
        cell_dimensions,
        hyperlinks,
        ids: ids.into_iter().collect(),
    }
}

fn component(
    data: &str,
    mime: &str,
    options: ImageOptions,
    explicit_dimensions: Option<ImageDimensions>,
    spec: EnvironmentSpec,
) -> (Image, FactsHandle) {
    let (environment, handle) = environment(
        spec.protocol,
        spec.cell_dimensions,
        spec.hyperlinks,
        spec.ids,
    );
    (
        Image::with_environment(
            data,
            mime,
            theme(),
            options,
            explicit_dimensions,
            environment,
        ),
        handle,
    )
}

#[test]
fn oracle_is_exactly_pinned_and_non_vacuous() {
    let oracle = &fixture()["oracle"];
    assert_eq!(oracle["package"], "@earendil-works/pi-tui");
    assert_eq!(oracle["version"], "0.84.1");
    assert_eq!(
        oracle["files"]["components/image.d.ts"],
        "45cfb14d766704c70017d7ec3a2d382f148fbf56b7f76c4c3155cc80bb5ff6cb"
    );
    assert_eq!(
        oracle["files"]["components/image.js"],
        "dd6791e17fbeb0a48c2b73d521d31356edf11795e44e0fae05b5f8c322c470e1"
    );
    assert_eq!(
        oracle["files"]["terminal-image.d.ts"],
        "ba498675c6f16339fe04c329dcd95757743f0f6d22a18879b2fda6e9e8b4d8ec"
    );
    assert_eq!(
        oracle["files"]["terminal-image.js"],
        "b2b60272f6f25dbb559722dffa369944f5c8846fa726c31fb841d1532470adc2"
    );
    assert_eq!(
        oracle["files"]["utils.d.ts"],
        "1c68478346b8451cc61c7dd6cb35f226ae8011117be85a6b3f3cffbb898242d2"
    );
    assert_eq!(
        oracle["files"]["utils.js"],
        "70c037e8c3c6ec909c4bab6b14777e1f33ab1f5c39f5f1f3aa6f8966357d8052"
    );
    assert_eq!(fixture()["dimensionPriority"].as_array().unwrap().len(), 4);
    assert_eq!(fixture()["formats"].as_array().unwrap().len(), 6);
    assert_eq!(fixture()["boundaries"].as_array().unwrap().len(), 8);
    assert_eq!(fixture()["kitty"]["randomCalls"], 1);
    assert_eq!(fixture()["providedId"]["randomCalls"], 0);
    assert_eq!(fixture()["providedZeroId"]["randomCalls"], 0);
    assert_eq!(fixture()["defaultLimits"]["randomCalls"], 0);

    for (group, key) in [
        ("kitty", "sameWidthSameReference"),
        ("kitty", "widthMissNewReference"),
        ("cache", "sameWidthSameReference"),
        ("cache", "factChangeSameReference"),
        ("cache", "widthMissNewReference"),
        ("cache", "changedFactsSameWidthReference"),
        ("cache", "invalidateNewReference"),
        ("fallbackCache", "factChangeSameReference"),
        ("fallbackCache", "invalidateNewReference"),
    ] {
        assert_eq!(fixture()[group][key], true, "{group}.{key}");
    }
}

#[test]
fn explicit_parsed_and_default_dimensions_match_all_formats() {
    for case in fixture()["dimensionPriority"].as_array().unwrap() {
        let explicit = if case["explicitDimensions"].is_null() {
            None
        } else {
            Some(fixture_dimensions(&case["explicitDimensions"]))
        };
        let (mut image, _) = component(
            case["data"].as_str().unwrap(),
            case["mime"].as_str().unwrap(),
            ImageOptions::default(),
            explicit,
            environment_spec(None, cells(9.0, 18.0), false, []),
        );
        assert_eq!(image.dimensions(), fixture_dimensions(&case["dimensions"]));
        assert_eq!(image.render(80), lines(&case["lines"]), "{}", case["label"]);
    }

    for case in fixture()["formats"].as_array().unwrap() {
        let (mut image, _) = component(
            case["data"].as_str().unwrap(),
            case["mime"].as_str().unwrap(),
            ImageOptions::default(),
            None,
            environment_spec(None, cells(9.0, 18.0), false, []),
        );
        assert_eq!(image.dimensions(), fixture_dimensions(&case["dimensions"]));
        assert_eq!(image.render(80), lines(&case["lines"]), "{}", case["label"]);
    }
}

#[test]
fn kitty_layout_id_cache_and_ownership_match() {
    let expected = &fixture()["kitty"];
    let id = expected["imageId"].as_u64().unwrap() as u32;
    let (mut image, handle) = component(
        "QUJD",
        "image/png",
        ImageOptions::default(),
        Some(dimensions(100, 50)),
        environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, [id]),
    );

    assert_eq!(image.render(20), lines(&expected["width20"]));
    let first_cache = image.cache_stats();
    assert_eq!(image.render(20), lines(&expected["width20Again"]));
    assert_eq!(
        image.cache_stats().allocation_generation,
        first_cache.allocation_generation
    );
    assert_eq!(image.render(8), lines(&expected["width8"]));
    assert_eq!(image.get_image_id(), Some(id));
    assert_eq!(handle.allocation_calls(), 1);
    assert_eq!(
        image.cache_stats(),
        ImageCacheStats {
            hits: 1,
            misses: 2,
            allocation_generation: 2,
            cached_width: Some(8),
            cached_line_count: 2,
        }
    );

    let ownership = image.kitty_image_ownership().expect("Kitty ownership");
    assert_eq!(ownership.metadata.image_id, id);
    assert_eq!(ownership.metadata.columns, 6);
    assert_eq!(ownership.metadata.rows, 2);
    assert_eq!(ownership.metadata.width_px, 100);
    assert_eq!(ownership.metadata.height_px, 50);
    assert_eq!(ownership.deletion_owner, KittyImageDeletionOwner::Component);
    drop(image);
    assert_eq!(
        handle.allocation_calls(),
        1,
        "Drop performs no terminal work"
    );
}

#[test]
fn iterm_order_and_all_numeric_boundaries_match() {
    let (mut iterm, handle) = component(
        "QUJD",
        "image/png",
        ImageOptions {
            max_width_cells: Some(5.0),
            ..ImageOptions::default()
        },
        Some(dimensions(100, 50)),
        environment_spec(Some(ImageProtocol::ITerm2), cells(10.0, 20.0), false, []),
    );
    assert_eq!(iterm.render(20), lines(&fixture()["iterm2"]["maxWidth5"]));
    assert_eq!(iterm.get_image_id(), None);
    assert_eq!(handle.allocation_calls(), 0);

    for case in fixture()["boundaries"].as_array().unwrap() {
        let label = case["label"].as_str().unwrap();
        let mut options = ImageOptions {
            image_id: Some(77),
            ..ImageOptions::default()
        };
        match label {
            "max-width-zero" => options.max_width_cells = Some(0.0),
            "max-width-negative" => options.max_width_cells = Some(-5.0),
            "explicit-max-height-one" => options.max_height_cells = Some(1.0),
            "explicit-max-height-two" => options.max_height_cells = Some(2.0),
            _ => {}
        }
        let (mut image, handle) = component(
            "QUJD",
            "image/png",
            options,
            Some(dimensions(100, 50)),
            environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, []),
        );
        assert_eq!(
            image.render(case["width"].as_u64().unwrap() as usize),
            lines(&case["lines"]),
            "{label}"
        );
        assert_eq!(handle.allocation_calls(), 0, "provided ID: {label}");
    }
}

#[test]
fn fallback_is_themed_then_terminal_width_truncated() {
    let (mut plain, _) = component(
        "!!!!",
        "image/png",
        ImageOptions {
            filename: Some("/opt/pi-home/pictures/very-long-image-name.png".to_owned()),
            ..ImageOptions::default()
        },
        None,
        environment_spec(None, cells(10.0, 20.0), false, []),
    );
    assert_eq!(
        plain.render(16),
        lines(&fixture()["fallback"]["styledTruncated"])
    );

    let png = fixture()["formats"][0]["data"].as_str().unwrap();
    let (mut linked, _) = component(
        png,
        "image/png",
        ImageOptions {
            filename: Some("/opt/pi-home/pictures/a b.png".to_owned()),
            ..ImageOptions::default()
        },
        Some(dimensions(3, 4)),
        environment_spec(None, cells(10.0, 20.0), true, []),
    );
    assert_eq!(
        linked.render(32),
        lines(&fixture()["fallback"]["styledHyperlinkTruncated"])
    );

    let (mut empty_filename, _) = component(
        "!!!!",
        "image/png",
        ImageOptions {
            filename: Some(String::new()),
            ..ImageOptions::default()
        },
        Some(dimensions(3, 4)),
        environment_spec(None, cells(10.0, 20.0), false, []),
    );
    assert_eq!(
        empty_filename.render(80),
        lines(&fixture()["fallback"]["emptyFilename"]),
        "an empty filename is omitted instead of adding a fallback space"
    );
}

#[test]
fn exact_width_is_the_only_cache_key_and_invalidate_refreshes_facts() {
    let expected = &fixture()["cache"];
    let id = expected["imageId"].as_u64().unwrap() as u32;
    let (mut image, handle) = component(
        "QUJD",
        "image/png",
        ImageOptions::default(),
        Some(dimensions(100, 50)),
        environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, [id]),
    );

    assert_eq!(image.render(20), lines(&expected["first"]));
    let first_generation = image.cache_stats().allocation_generation;
    assert_eq!(image.render(20), lines(&expected["second"]));
    assert_eq!(image.cache_stats().allocation_generation, first_generation);

    handle.set(Some(ImageProtocol::ITerm2), cells(5.0, 10.0), false);
    assert_eq!(image.render(20), lines(&expected["staleAfterFactChange"]));
    assert_eq!(image.cache_stats().allocation_generation, first_generation);
    assert_eq!(image.render(19), lines(&expected["widthMiss"]));
    let second_generation = image.cache_stats().allocation_generation;
    assert!(second_generation > first_generation);

    handle.set(None, cells(99.0, 101.0), false);
    assert_eq!(image.render(19), lines(&expected["staleWidthMiss"]));
    assert_eq!(image.cache_stats().allocation_generation, second_generation);
    image.invalidate();
    assert_eq!(image.cache_stats().cached_width, None);
    assert_eq!(
        image.render(19),
        lines(&expected["refreshedAfterInvalidate"])
    );
    assert_eq!(
        image.cache_stats().allocation_generation,
        second_generation + 1
    );
    assert_eq!(handle.allocation_calls(), 1);
}

#[test]
fn fallback_cache_refresh_and_provided_id_ownership_match() {
    let expected = &fixture()["fallbackCache"];
    let id = expected["imageId"].as_u64().unwrap() as u32;
    let (mut image, handle) = component(
        "QUJD",
        "image/png",
        ImageOptions::default(),
        Some(dimensions(100, 50)),
        environment_spec(None, cells(10.0, 20.0), false, [id]),
    );
    assert_eq!(image.render(20), lines(&expected["first"]));
    handle.set(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false);
    assert_eq!(image.render(20), lines(&expected["stale"]));
    assert_eq!(handle.allocation_calls(), 0);
    image.invalidate();
    assert_eq!(image.render(20), lines(&expected["refreshed"]));
    assert_eq!(handle.allocation_calls(), 1);

    let provided = &fixture()["providedId"];
    let provided_id = provided["imageId"].as_u64().unwrap() as u32;
    let (mut reused, handle) = component(
        "QUJD",
        "image/png",
        ImageOptions {
            image_id: Some(provided_id),
            ..ImageOptions::default()
        },
        Some(dimensions(100, 50)),
        environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, []),
    );
    assert_eq!(reused.render(20), lines(&provided["lines"]));
    assert_eq!(handle.allocation_calls(), 0);
    assert_eq!(
        reused.kitty_image_ownership().unwrap().deletion_owner,
        KittyImageDeletionOwner::Caller
    );
}

#[test]
fn caller_zero_id_is_preserved_without_allocation_transmission_or_ownership() {
    let expected = &fixture()["providedZeroId"];
    let (mut image, handle) = component(
        "QUJD",
        "image/png",
        ImageOptions {
            image_id: Some(0),
            ..ImageOptions::default()
        },
        Some(dimensions(100, 50)),
        environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, []),
    );

    assert_eq!(image.render(20), lines(&expected["lines"]));
    assert_eq!(image.get_image_id(), Some(0));
    assert_eq!(handle.allocation_calls(), 0);
    assert_eq!(image.kitty_image_ownership(), None);
}

#[test]
fn default_width_and_cell_aspect_height_limits_match() {
    let expected = &fixture()["defaultLimits"];
    let (mut wide, wide_handle) = component(
        "QUJD",
        "image/png",
        ImageOptions {
            image_id: Some(77),
            ..ImageOptions::default()
        },
        Some(dimensions(100, 50)),
        environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, []),
    );
    assert_eq!(wide.render(100), lines(&expected["width100"]));
    assert_eq!(wide_handle.allocation_calls(), 0);

    let (mut tall, tall_handle) = component(
        "QUJD",
        "image/png",
        ImageOptions {
            image_id: Some(77),
            ..ImageOptions::default()
        },
        Some(dimensions(100, 1000)),
        environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, []),
    );
    assert_eq!(tall.render(22), lines(&expected["tallWidth22"]));
    assert_eq!(tall_handle.allocation_calls(), 0);
}

#[test]
fn maximum_headers_flow_through_overflow_safe_image_math() {
    let expected = &fixture()["hugeDimensions"];
    let width = expected["width"].as_u64().unwrap() as usize;
    let (mut image, handle) = component(
        "iVBORwAAAAAAAAAAAAAAAP//////////",
        "image/png",
        ImageOptions {
            max_width_cells: Some(width as f64),
            max_height_cells: Some(1.0),
            image_id: Some(0xffff_fffe),
            ..ImageOptions::default()
        },
        Some(dimensions(u32::MAX, u32::MAX)),
        environment_spec(Some(ImageProtocol::Kitty), cells(10.0, 20.0), false, []),
    );
    assert_eq!(image.render(width), lines(&expected["lines"]));
    assert_eq!(handle.allocation_calls(), 0);
}
