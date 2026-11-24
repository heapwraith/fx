use crate::{AssetSource, DevicePixels, IsZero, Result, SharedString, Size};
#[cfg(feature = "svg")]
use resvg::tiny_skia::Pixmap;
#[cfg(feature = "svg")]
use std::sync::LazyLock;
use std::{hash::Hash, sync::Arc};

// fx patch: SVG rendering (usvg, resvg, tiny-skia and their font stack) is
// behind the `svg` feature, on by default as upstream. fx draws no SVG and
// turns it off: about 1 MB less in the program (T5.7). Without it every SVG
// fails to render with `SvgError`, and nothing else changes.
/// The error of a failed SVG render.
#[cfg(feature = "svg")]
pub type SvgError = usvg::Error;

/// The error of a failed SVG render: SVG support is not compiled in.
#[cfg(not(feature = "svg"))]
#[derive(Debug, Clone, Copy)]
pub struct SvgError;

#[cfg(not(feature = "svg"))]
impl std::fmt::Display for SvgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SVG support is not compiled in (gpui feature \"svg\")")
    }
}

#[cfg(not(feature = "svg"))]
impl std::error::Error for SvgError {}

/// Stands in for the rendered pixmap when SVG support is off (never built).
#[cfg(not(feature = "svg"))]
pub struct Pixmap(std::convert::Infallible);

#[cfg(not(feature = "svg"))]
impl Pixmap {
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        match self.0 {}
    }
    /// Height in pixels.
    pub fn height(&self) -> u32 {
        match self.0 {}
    }
    /// The pixel data.
    pub fn take(self) -> Vec<u8> {
        match self.0 {}
    }
}

/// When rendering SVGs, we render them at twice the size to get a higher-quality result.
pub const SMOOTH_SVG_SCALE_FACTOR: f32 = 2.;

#[derive(Clone, PartialEq, Hash, Eq)]
pub(crate) struct RenderSvgParams {
    pub(crate) path: SharedString,
    pub(crate) size: Size<DevicePixels>,
}

#[derive(Clone)]
pub struct SvgRenderer {
    asset_source: Arc<dyn AssetSource>,
    #[cfg(feature = "svg")]
    usvg_options: Arc<usvg::Options<'static>>,
}

pub enum SvgSize {
    Size(Size<DevicePixels>),
    ScaleFactor(f32),
}

impl SvgRenderer {
    #[cfg(not(feature = "svg"))]
    pub fn new(asset_source: Arc<dyn AssetSource>) -> Self {
        Self { asset_source }
    }

    #[cfg(feature = "svg")]
    pub fn new(asset_source: Arc<dyn AssetSource>) -> Self {
        static FONT_DB: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        });
        let default_font_resolver = usvg::FontResolver::default_font_selector();
        let font_resolver = Box::new(
            move |font: &usvg::Font, db: &mut Arc<usvg::fontdb::Database>| {
                if db.is_empty() {
                    *db = FONT_DB.clone();
                }
                default_font_resolver(font, db)
            },
        );
        let options = usvg::Options {
            font_resolver: usvg::FontResolver {
                select_font: font_resolver,
                select_fallback: usvg::FontResolver::default_fallback_selector(),
            },
            ..Default::default()
        };
        Self {
            asset_source,
            usvg_options: Arc::new(options),
        }
    }

    pub(crate) fn render(
        &self,
        params: &RenderSvgParams,
    ) -> Result<Option<(Size<DevicePixels>, Vec<u8>)>> {
        anyhow::ensure!(!params.size.is_zero(), "can't render at a zero size");

        // Load the tree.
        let Some(bytes) = self.asset_source.load(&params.path)? else {
            return Ok(None);
        };

        let pixmap = self.render_pixmap(&bytes, SvgSize::Size(params.size))?;
        Ok(Some(Self::alpha_mask(pixmap)))
    }

    #[cfg(not(feature = "svg"))]
    fn alpha_mask(pixmap: Pixmap) -> (Size<DevicePixels>, Vec<u8>) {
        match pixmap.0 {}
    }

    #[cfg(feature = "svg")]
    fn alpha_mask(pixmap: Pixmap) -> (Size<DevicePixels>, Vec<u8>) {
        // Convert the pixmap's pixels into an alpha mask.
        let size = Size::new(
            DevicePixels(pixmap.width() as i32),
            DevicePixels(pixmap.height() as i32),
        );
        let alpha_mask = pixmap
            .pixels()
            .iter()
            .map(|p| p.alpha())
            .collect::<Vec<_>>();
        (size, alpha_mask)
    }

    #[cfg(not(feature = "svg"))]
    pub fn render_pixmap(&self, _bytes: &[u8], _size: SvgSize) -> Result<Pixmap, SvgError> {
        Err(SvgError)
    }

    #[cfg(feature = "svg")]
    pub fn render_pixmap(&self, bytes: &[u8], size: SvgSize) -> Result<Pixmap, SvgError> {
        let tree = usvg::Tree::from_data(bytes, &self.usvg_options)?;
        let svg_size = tree.size();
        let scale = match size {
            SvgSize::Size(size) => size.width.0 as f32 / svg_size.width(),
            SvgSize::ScaleFactor(scale) => scale,
        };

        // Render the SVG to a pixmap with the specified width and height.
        let mut pixmap = resvg::tiny_skia::Pixmap::new(
            (svg_size.width() * scale) as u32,
            (svg_size.height() * scale) as u32,
        )
        .ok_or(usvg::Error::InvalidSize)?;

        let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Ok(pixmap)
    }
}
