//! Image quick actions (T6.16, D-068): a copy of an image in another format
//! or at a smaller size, with the Windows Imaging Component (the codecs
//! Windows already has: JPEG, PNG, GIF, BMP, TIFF, HEIC with its extension…).
//! The original is never touched; run on a worker thread.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
    IWICBitmapFrameEncode, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapEncoderNoCache, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::StructuredStorage::IPropertyBag2;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::{GUID, HSTRING, Interface};

/// The format of the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
}

impl ImageKind {
    pub fn extension(self) -> &'static str {
        match self {
            ImageKind::Png => "png",
            ImageKind::Jpeg => "jpg",
        }
    }
}

/// Writes `src` as `kind` to `dst` (created or replaced: the caller picks a
/// name nobody uses), scaled by `scale` (0 < scale ≤ 1). A failed write
/// leaves nothing at `dst`.
pub fn convert_image(src: &Path, dst: &Path, kind: ImageKind, scale: f32) -> io::Result<()> {
    let result = write(src, dst, kind, scale.clamp(0.01, 1.0));
    if result.is_err() {
        let _ = std::fs::remove_file(dst);
    }
    result
}

fn write(src: &Path, dst: &Path, kind: ImageKind, scale: f32) -> io::Result<()> {
    // SAFETY: every call below gets valid interfaces created here and
    // NUL-terminated strings kept alive by HSTRING; COM is initialized for
    // this (worker) thread first.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromFilename(
            &HSTRING::from(src.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h)?;
        let (nw, nh) = (
            ((w as f32 * scale).round() as u32).max(1),
            ((h as f32 * scale).round() as u32).max(1),
        );
        let mut source: IWICBitmapSource = frame.cast()?;
        if (nw, nh) != (w, h) {
            let scaler = factory.CreateBitmapScaler()?;
            scaler.Initialize(&source, nw, nh, WICBitmapInterpolationModeFant)?;
            source = scaler.cast()?;
        }
        let stream = factory.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(dst.as_os_str()), GENERIC_WRITE.0)?;
        let container: GUID = match kind {
            ImageKind::Png => GUID_ContainerFormatPng,
            ImageKind::Jpeg => GUID_ContainerFormatJpeg,
        };
        let encoder = factory.CreateEncoder(&container, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut target: Option<IWICBitmapFrameEncode> = None;
        let mut options: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut target, &mut options)?;
        let target = target.ok_or_else(|| io::Error::other("no frame to encode"))?;
        target.Initialize(options.as_ref())?;
        target.SetSize(nw, nh)?;
        // The encoder says which pixel format it takes; convert to it.
        let mut format = source.GetPixelFormat()?;
        target.SetPixelFormat(&mut format)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &source,
            &format,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        target.WriteSource(&converter.cast::<IWICBitmapSource>()?, std::ptr::null())?;
        target.Commit()?;
        encoder.Commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×2 BMP written by hand, converted to PNG and to a half-size JPEG.
    #[test]
    fn converts_and_scales() {
        let dir = tempfile::tempdir().unwrap();
        let bmp = dir.path().join("x.bmp");
        let (w, h) = (4u32, 2u32);
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&size.to_le_bytes());
        b.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        b.extend_from_slice(&[1, 0, 24, 0]);
        b.extend_from_slice(&[0; 24]);
        b.extend(std::iter::repeat_n(0x80u8, (row * h) as usize));
        std::fs::write(&bmp, &b).unwrap();
        let png = dir.path().join("x.png");
        convert_image(&bmp, &png, ImageKind::Png, 1.0).unwrap();
        assert_eq!(&std::fs::read(&png).unwrap()[1..4], b"PNG");
        let jpg = dir.path().join("x.jpg");
        convert_image(&bmp, &jpg, ImageKind::Jpeg, 0.5).unwrap();
        assert_eq!(&std::fs::read(&jpg).unwrap()[..2], &[0xFF, 0xD8]);
        let bad = dir.path().join("bad.png");
        assert!(convert_image(&dir.path().join("none.bmp"), &bad, ImageKind::Png, 1.0).is_err());
        assert!(!bad.exists(), "nothing left behind");
    }
}
