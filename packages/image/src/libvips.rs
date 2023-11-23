use bytes::Bytes;
use libvips::{ops, VipsApp, VipsImage};
use log::debug;
use once_cell::sync::Lazy;

static VIPS: Lazy<VipsApp> = Lazy::new(|| {
    debug!("Initializing libvips");
    let app = VipsApp::new("Moosicbox Libvips", false).expect("Cannot initialize libvips");

    app.concurrency_set(4);

    app
});

pub fn get_error() -> String {
    let error = VIPS.error_buffer().unwrap_or_default().to_string();
    VIPS.error_clear();
    error
}

pub fn resize_local_file(
    width: u32,
    height: u32,
    path: &str,
) -> Result<Bytes, libvips::error::Error> {
    let _app = &VIPS;
    let options = ops::ThumbnailImageOptions {
        height: height as i32,
        import_profile: "sRGB".into(),
        export_profile: "sRGB".into(),
        ..ops::ThumbnailImageOptions::default()
    };

    let image = VipsImage::new_from_file(path)?;

    let thumbnail = ops::thumbnail_image_with_opts(&image, width as i32, &options)?;
    let buffer = thumbnail.image_write_to_buffer("image.jpeg")?;

    Ok(buffer.into())
}

pub fn resize_bytes(width: u32, height: u32, bytes: &[u8]) -> Result<Bytes, libvips::error::Error> {
    let _app = &VIPS;
    let options = ops::ThumbnailBufferOptions {
        height: height as i32,
        import_profile: "sRGB".into(),
        export_profile: "sRGB".into(),
        ..ops::ThumbnailBufferOptions::default()
    };

    let thumbnail = ops::thumbnail_buffer_with_opts(bytes, width as i32, &options)?;
    let buffer = thumbnail.image_write_to_buffer("image.jpeg")?;

    Ok(buffer.into())
}
