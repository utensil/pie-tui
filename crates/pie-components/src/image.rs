//! Inline-image component with injected terminal facts.
//!
//! This is the rank-2 port of the pinned `components/image.js`. Protocol
//! encoding, dimension parsing, and scaling stay in `pie-core`; capability
//! facts use the rank-1 `pie-term` shape. The default constructor is
//! deliberately conservative (text fallback only). Runtimes that want inline
//! images inject capability, cell-size, home-directory, and nonzero ID facts
//! through [`ImageEnvironment`].

use std::num::NonZeroU32;
use std::sync::Arc;

use pie_core::terminal_image::{
    CellDimensions, ImageDimensions, ImageProtocol, ImageRenderOptions, KittyImageMetadata,
    get_image_dimensions, image_fallback, render_image,
};
use pie_core::wrap::truncate_to_width;
use pie_term::capabilities::TerminalCapabilities;

use crate::{Component, StyleFn};

/// Theme callback used for the text fallback.
pub struct ImageTheme {
    pub fallback_color: StyleFn,
}

impl ImageTheme {
    pub fn new(fallback_color: impl Fn(&str) -> String + Send + 'static) -> Self {
        Self {
            fallback_color: Box::new(fallback_color),
        }
    }

    fn fallback(&self, value: &str) -> String {
        (self.fallback_color)(value)
    }
}

/// Optional limits and identity for one image.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageOptions {
    pub max_width_cells: Option<f64>,
    pub max_height_cells: Option<f64>,
    pub filename: Option<String>,
    /// A caller-owned Kitty image ID reused for animation or updates.
    pub image_id: Option<u32>,
}

/// Injected facts whose changes are intentionally ignored on a same-width
/// cache hit, matching the reference component.
pub trait ImageEnvironment: Send {
    fn capabilities(&self) -> TerminalCapabilities;
    fn cell_dimensions(&self) -> CellDimensions;
    fn allocate_image_id(&mut self) -> NonZeroU32;
    fn home_dir(&self) -> String;
}

/// Which layer must eventually issue the Kitty deletion sequence.
///
/// `Image` never writes terminal I/O, including during `Drop`. Main/alternate
/// screen runtimes can inspect [`Image::kitty_image_ownership`] and perform
/// deterministic cleanup at their own synchronized-output boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyImageDeletionOwner {
    /// The component lazily allocated the ID; its hosting runtime owns cleanup.
    Component,
    /// The caller supplied a reused ID and retains cleanup responsibility.
    Caller,
}

/// Last Kitty placement plus explicit deletion responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KittyImageOwnership {
    pub metadata: KittyImageMetadata,
    pub deletion_owner: KittyImageDeletionOwner,
}

/// Observable cache accounting for the Rust ownership adapter.
///
/// The JavaScript implementation returns the identical cached Array object.
/// `Component::render` must return an owned `Vec<String>`, so Rust callers get a
/// clone. `allocation_generation` proves whether the single internal `Arc`
/// allocation was retained without claiming cross-language object identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub allocation_generation: u64,
    pub cached_width: Option<usize>,
    pub cached_line_count: usize,
}

struct CachedLines {
    width: usize,
    lines: Arc<[String]>,
}

struct ConservativeEnvironment;

impl ImageEnvironment for ConservativeEnvironment {
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            images: None,
            true_color: false,
            hyperlinks: false,
        }
    }

    fn cell_dimensions(&self) -> CellDimensions {
        CellDimensions::default()
    }

    fn allocate_image_id(&mut self) -> NonZeroU32 {
        // This environment never reports Kitty. Returning the minimum nonzero
        // value keeps the trait total without introducing hidden entropy.
        NonZeroU32::MIN
    }

    fn home_dir(&self) -> String {
        String::new()
    }
}

/// Inline-image component with exact-width caching.
pub struct Image {
    base64_data: String,
    mime_type: String,
    dimensions: ImageDimensions,
    theme: ImageTheme,
    options: ImageOptions,
    image_id: Option<u32>,
    deletion_owner: Option<KittyImageDeletionOwner>,
    kitty_ownership: Option<KittyImageOwnership>,
    environment: Box<dyn ImageEnvironment>,
    cache: Option<CachedLines>,
    cache_stats: ImageCacheStats,
}

impl Image {
    /// Construct a text-fallback image with default options.
    ///
    /// Inline-image runtimes use [`Self::with_environment`] so environment and
    /// ID entropy remain explicit.
    pub fn new(
        base64_data: impl Into<String>,
        mime_type: impl Into<String>,
        theme: ImageTheme,
    ) -> Self {
        Self::with_environment(
            base64_data,
            mime_type,
            theme,
            ImageOptions::default(),
            None,
            Box::new(ConservativeEnvironment),
        )
    }

    /// Construct with canonical options and conservative terminal facts.
    pub fn with_options(
        base64_data: impl Into<String>,
        mime_type: impl Into<String>,
        theme: ImageTheme,
        options: ImageOptions,
        dimensions: Option<ImageDimensions>,
    ) -> Self {
        Self::with_environment(
            base64_data,
            mime_type,
            theme,
            options,
            dimensions,
            Box::new(ConservativeEnvironment),
        )
    }

    /// Construct with all mutable host facts injected behind a narrow seam.
    pub fn with_environment(
        base64_data: impl Into<String>,
        mime_type: impl Into<String>,
        theme: ImageTheme,
        options: ImageOptions,
        dimensions: Option<ImageDimensions>,
        environment: Box<dyn ImageEnvironment>,
    ) -> Self {
        let base64_data = base64_data.into();
        let mime_type = mime_type.into();
        let dimensions = dimensions
            .or_else(|| get_image_dimensions(&base64_data, &mime_type))
            .unwrap_or(ImageDimensions {
                width_px: 800,
                height_px: 600,
            });
        let image_id = options.image_id;
        let deletion_owner = image_id
            .filter(|image_id| *image_id != 0)
            .map(|_| KittyImageDeletionOwner::Caller);
        Self {
            base64_data,
            mime_type,
            dimensions,
            theme,
            options,
            image_id,
            deletion_owner,
            kitty_ownership: None,
            environment,
            cache: None,
            cache_stats: ImageCacheStats::default(),
        }
    }

    pub fn dimensions(&self) -> ImageDimensions {
        self.dimensions
    }

    /// Get the lazily allocated or caller-provided Kitty image ID.
    pub fn get_image_id(&self) -> Option<u32> {
        self.image_id
    }

    /// Last Kitty placement and the layer responsible for deleting its ID.
    pub fn kitty_image_ownership(&self) -> Option<KittyImageOwnership> {
        self.kitty_ownership
    }

    pub fn cache_stats(&self) -> ImageCacheStats {
        ImageCacheStats {
            cached_width: self.cache.as_ref().map(|cache| cache.width),
            cached_line_count: self.cache.as_ref().map_or(0, |cache| cache.lines.len()),
            ..self.cache_stats
        }
    }

    fn render_cached(&mut self, width: usize) -> Arc<[String]> {
        if let Some(cache) = &self.cache
            && cache.width == width
        {
            self.cache_stats.hits = self.cache_stats.hits.saturating_add(1);
            return Arc::clone(&cache.lines);
        }

        self.cache_stats.misses = self.cache_stats.misses.saturating_add(1);
        let max_width = ((width as f64) - 2.0)
            .min(self.options.max_width_cells.unwrap_or(60.0))
            .max(1.0);
        let cell_dimensions = self.environment.cell_dimensions();
        let default_max_height = ((max_width * cell_dimensions.width_px)
            / cell_dimensions.height_px)
            .ceil()
            .max(1.0);
        let max_height = self.options.max_height_cells.unwrap_or(default_max_height);
        let capabilities = self.environment.capabilities();

        let rendered = if let Some(protocol) = capabilities.images {
            if protocol == ImageProtocol::Kitty && self.image_id.is_none() {
                self.image_id = Some(self.environment.allocate_image_id().get());
                self.deletion_owner = Some(KittyImageDeletionOwner::Component);
            }
            render_image(
                &self.base64_data,
                self.dimensions,
                Some(protocol),
                &ImageRenderOptions {
                    max_width_cells: Some(max_width),
                    max_height_cells: Some(max_height),
                    image_id: self.image_id,
                    move_cursor: Some(false),
                    ..ImageRenderOptions::default()
                },
                cell_dimensions,
            )
            .map(|result| {
                if let Some(image_id) = result.image_id {
                    self.image_id = Some(image_id);
                }
                if protocol == ImageProtocol::Kitty
                    && let (Some(image_id), Some(deletion_owner)) =
                        (self.image_id, self.deletion_owner)
                {
                    self.kitty_ownership = Some(KittyImageOwnership {
                        metadata: KittyImageMetadata {
                            image_id,
                            columns: result.columns,
                            rows: result.rows,
                            width_px: self.dimensions.width_px,
                            height_px: self.dimensions.height_px,
                        },
                        deletion_owner,
                    });
                }
                image_lines(protocol, result.sequence, result.rows)
            })
        } else {
            None
        }
        .unwrap_or_else(|| {
            let fallback = image_fallback(
                &self.mime_type,
                Some(self.dimensions),
                self.options
                    .filename
                    .as_deref()
                    .filter(|filename| !filename.is_empty()),
                &self.environment.home_dir(),
                capabilities.hyperlinks,
            );
            vec![truncate_to_width(
                &self.theme.fallback(&fallback),
                width,
                "...",
                false,
            )]
        });

        let lines: Arc<[String]> = rendered.into();
        self.cache_stats.allocation_generation =
            self.cache_stats.allocation_generation.saturating_add(1);
        self.cache = Some(CachedLines {
            width,
            lines: Arc::clone(&lines),
        });
        lines
    }
}

fn image_lines(protocol: ImageProtocol, sequence: String, rows: usize) -> Vec<String> {
    let reserved_rows = rows.saturating_sub(1);
    match protocol {
        ImageProtocol::Kitty => {
            let mut lines = Vec::with_capacity(rows);
            lines.push(sequence);
            lines.resize(rows, String::new());
            lines
        }
        ImageProtocol::ITerm2 => {
            let mut lines = vec![String::new(); reserved_rows];
            let move_up = if reserved_rows > 0 {
                format!("\x1b[{reserved_rows}A")
            } else {
                String::new()
            };
            lines.push(format!("{move_up}{sequence}"));
            lines
        }
    }
}

impl Component for Image {
    fn invalidate(&mut self) {
        self.cache = None;
    }

    fn render(&mut self, width: usize) -> Vec<String> {
        self.render_cached(width).iter().cloned().collect()
    }
}
