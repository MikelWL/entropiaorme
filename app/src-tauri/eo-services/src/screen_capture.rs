//! Screen-region capture for OCR: the equivalent of the original
//! Python implementation's mss-based capturer. A region (`x`, `y`, `w`, `h`) goes in; PNG bytes (the
//! skill-scan path, RGB-encoded to match `mss.tools.to_png(shot.rgb)`) or
//! a [`BgrImage`] (the repair-OCR path) comes out, via a GDI `BitBlt` of
//! the screen device context.
//!
//! Windows-only: off Windows the captures return `None`, exactly as the
//! OCR engine and the keystroke hook stand down, so the scan/repair
//! routes report "engine unavailable" rather than serving an empty capture.
//! Capture is on-demand with no persistent handle (mirroring the Python
//! capturer's per-call grab), so there is nothing to leak between scans.

use image::ImageEncoder;

use crate::skill_panel::BgrImage;

/// Capture a screen rectangle as PNG bytes (RGB-encoded). `None` on a
/// non-positive region, a capture failure, or a non-Windows host.
pub fn capture_region_png(x: i64, y: i64, w: i64, h: i64) -> Option<Vec<u8>> {
    let bgra = platform::capture_bgra(x, y, w, h)?;
    let (pw, ph) = (w as u32, h as u32);
    // BGRA (top-down) -> RGB, the order `mss.tools.to_png(shot.rgb)` encodes.
    let mut rgb = Vec::with_capacity((pw as usize) * (ph as usize) * 3);
    for px in bgra.chunks_exact(4) {
        rgb.push(px[2]);
        rgb.push(px[1]);
        rgb.push(px[0]);
    }
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(&rgb, pw, ph, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(out)
}

/// Capture a screen rectangle as a BGR image (the repair-OCR path). `None`
/// on a non-positive region, a capture failure, or a non-Windows host.
pub fn capture_region_bgr(x: i64, y: i64, w: i64, h: i64) -> Option<BgrImage> {
    let bgra = platform::capture_bgra(x, y, w, h)?;
    // BGRA -> BGR (drop the alpha), matching the `[:, :, :3]` slice of the
    // Python capturer's BGR ndarray.
    let mut data = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for px in bgra.chunks_exact(4) {
        data.push(px[0]);
        data.push(px[1]);
        data.push(px[2]);
    }
    Some(BgrImage {
        data,
        h: h as usize,
        w: w as usize,
    })
}

#[cfg(windows)]
mod platform {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };

    /// Capture the rectangle as top-down 32-bit BGRA bytes via GDI. Every
    /// handle acquired is released on every path; the screen DC is released
    /// last. `None` on any GDI failure or a non-positive region.
    pub fn capture_bgra(x: i64, y: i64, w: i64, h: i64) -> Option<Vec<u8>> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let (x, y, w, h) = (x as i32, y as i32, w as i32, h as i32);
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let result = capture_into(screen, x, y, w, h);
            ReleaseDC(None, screen);
            result
        }
    }

    /// The inner GDI dance, factored so the screen DC release in the caller
    /// runs on every exit. SAFETY: called only with a valid screen DC; each
    /// created object is selected out and deleted before return.
    unsafe fn capture_into(
        screen: windows::Win32::Graphics::Gdi::HDC,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Option<Vec<u8>> {
        let mem = CreateCompatibleDC(Some(screen));
        if mem.is_invalid() {
            return None;
        }
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        if bitmap.is_invalid() {
            let _ = DeleteDC(mem);
            return None;
        }
        let previous = SelectObject(mem, HGDIOBJ::from(bitmap));
        let blitted = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY).is_ok();

        // A negative height requests a top-down DIB (row 0 is the top), so
        // the byte order matches the Python grab without a vertical flip.
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buffer = vec![0u8; (w as usize) * (h as usize) * 4];
        let rows = if blitted {
            GetDIBits(
                mem,
                bitmap,
                0,
                h as u32,
                Some(buffer.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        SelectObject(mem, previous);
        let _ = DeleteObject(HGDIOBJ::from(bitmap));
        let _ = DeleteDC(mem);

        if rows == h {
            Some(buffer)
        } else {
            None
        }
    }
}

/// The Linux capturer is a ScreenCast-portal + PipeWire pipeline, held
/// open for the process lifetime behind the same stateless per-call
/// contract. The Windows GDI path can grab any screen rectangle on
/// demand with no prior consent; Wayland cannot, so the first capture
/// lazily opens a consented monitor stream (reusing a persisted restore
/// token on later launches, so consent is asked at most once) and keeps
/// a background GStreamer pipeline pulling the latest full-monitor frame
/// into a shared slot. Each `capture_bgra` then crops that frame to the
/// requested screen rectangle, translating global screen coordinates to
/// frame-local ones through the stream's monitor position. A capture
/// before the stream has produced its first frame, or a rectangle
/// outside the captured monitor, returns None, exactly as the Windows
/// path returns None on a GDI failure.
///
/// The restore token lives under the app data dir; the module reads its
/// location from `EO_CAPTURE_TOKEN_PATH` (set by the shell at startup)
/// and skips persistence when unset, so a headless test never writes to
/// a real config location.
///
/// Gated behind the `linux-capture` feature: the GStreamer stack links
/// system libraries the backend members must not require, so the default
/// backend build stands the capturer down (returning `None`, exactly as
/// an unsupported host does) and only the shell enables the real path.
#[cfg(all(target_os = "linux", feature = "linux-capture"))]
mod platform {
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
    use ashpd::desktop::PersistMode;
    use gstreamer::prelude::*;
    use gstreamer_app::AppSink;

    /// The most recent full-monitor frame plus where that monitor sits in
    /// the global screen space, so a global rect can be cropped out of it.
    struct Frame {
        bgra: Vec<u8>,
        width: usize,
        height: usize,
        /// Monitor origin in global screen coordinates (the portal
        /// stream position).
        origin_x: i64,
        origin_y: i64,
    }

    struct Engine {
        latest: Arc<Mutex<Option<Frame>>>,
        // Kept alive for the process: dropping the pipeline tears down
        // the PipeWire stream, and dropping the session revokes the
        // portal grant.
        _pipeline: gstreamer::Pipeline,
    }

    fn engine() -> Option<&'static Engine> {
        static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();
        ENGINE.get_or_init(|| match start_engine() {
            Ok(engine) => Some(engine),
            Err(error) => {
                tracing::warn!(target: "eo::capture", %error, "screen capture engine unavailable");
                None
            }
        })
        .as_ref()
    }

    fn token_path() -> Option<std::path::PathBuf> {
        std::env::var_os("EO_CAPTURE_TOKEN_PATH").map(std::path::PathBuf::from)
    }

    /// Open the portal stream (blocking on a private runtime) and return
    /// the PipeWire node id plus the monitor origin and the restore token.
    fn open_portal() -> Result<(u32, i64, i64), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let saved = token_path()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty());

            let proxy = Screencast::new().await?;
            let session = proxy.create_session().await?;
            proxy
                .select_sources(
                    &session,
                    CursorMode::Hidden,
                    SourceType::Monitor.into(),
                    false,
                    saved.as_deref(),
                    PersistMode::ExplicitlyRevoked,
                )
                .await?;
            let response = proxy.start(&session, None).await?.response()?;

            if let Some(token) = response.restore_token() {
                if let Some(path) = token_path() {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    // Owner-only mode: the token is a capability at rest
                    // (it grants silent re-acquisition of the consented
                    // capture stream to a process presenting this app's
                    // identity), so it must not be readable by other users.
                    let _ = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(path)
                        .and_then(|mut file| {
                            use std::io::Write as _;
                            file.write_all(token.as_bytes())
                        });
                }
            }

            let stream = response
                .streams()
                .first()
                .ok_or("portal returned no stream")?
                .clone();
            let (ox, oy) = stream.position().unwrap_or((0, 0));
            // Leak the session so the grant survives for the process
            // lifetime; the OS reclaims it at exit.
            std::mem::forget(session);
            Ok::<_, Box<dyn std::error::Error>>((
                stream.pipe_wire_node_id(),
                i64::from(ox),
                i64::from(oy),
            ))
        })
    }

    fn start_engine() -> Result<Engine, Box<dyn std::error::Error>> {
        gstreamer::init()?;
        let (node_id, origin_x, origin_y) = open_portal()?;

        let latest: Arc<Mutex<Option<Frame>>> = Arc::new(Mutex::new(None));
        let pipeline = gstreamer::Pipeline::new();
        let src = gstreamer::ElementFactory::make("pipewiresrc")
            .property("path", node_id.to_string())
            .build()?;
        let convert = gstreamer::ElementFactory::make("videoconvert").build()?;
        let caps = gstreamer::Caps::builder("video/x-raw")
            .field("format", "BGRx")
            .build();
        let appsink = AppSink::builder()
            .caps(&caps)
            .max_buffers(1)
            .drop(true)
            .build();

        pipeline.add_many([&src, &convert, appsink.upcast_ref()])?;
        gstreamer::Element::link_many([&src, &convert, appsink.upcast_ref()])?;

        let sink_slot = latest.clone();
        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;
                    let caps = sample.caps().ok_or(gstreamer::FlowError::Error)?;
                    let info = gstreamer_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gstreamer::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gstreamer::FlowError::Error)?;
                    let map = buffer
                        .map_readable()
                        .map_err(|_| gstreamer::FlowError::Error)?;
                    let width = info.width() as usize;
                    let height = info.height() as usize;
                    // GStreamer rows are stride-padded; copy tight BGRA.
                    let stride = info.stride()[0] as usize;
                    let mut bgra = Vec::with_capacity(width * height * 4);
                    for row in 0..height {
                        let start = row * stride;
                        bgra.extend_from_slice(&map[start..start + width * 4]);
                    }
                    *sink_slot.lock().expect("frame slot") = Some(Frame {
                        bgra,
                        width,
                        height,
                        origin_x,
                        origin_y,
                    });
                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        pipeline.set_state(gstreamer::State::Playing)?;

        // Wait for the first frame so the initial capture does not race
        // the stream startup (bounded; a stream that never produces is a
        // failure, not a hang).
        let deadline = Instant::now() + Duration::from_secs(5);
        while latest.lock().expect("frame slot").is_none() {
            if Instant::now() > deadline {
                let _ = pipeline.set_state(gstreamer::State::Null);
                return Err("capture stream produced no frame within 5s".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Ok(Engine {
            latest,
            _pipeline: pipeline,
        })
    }

    pub fn capture_bgra(x: i64, y: i64, w: i64, h: i64) -> Option<Vec<u8>> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let engine = engine()?;
        let guard = engine.latest.lock().expect("frame slot");
        let frame = guard.as_ref()?;

        // Translate the global rectangle into frame-local coordinates.
        let local_x = x - frame.origin_x;
        let local_y = y - frame.origin_y;
        if local_x < 0
            || local_y < 0
            || (local_x + w) as usize > frame.width
            || (local_y + h) as usize > frame.height
        {
            return None;
        }

        let (w, h) = (w as usize, h as usize);
        let (local_x, local_y) = (local_x as usize, local_y as usize);
        let mut out = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            let start = ((local_y + row) * frame.width + local_x) * 4;
            out.extend_from_slice(&frame.bgra[start..start + w * 4]);
        }
        Some(out)
    }
}

/// The stand-down capturer: every host without a compiled capture backend
/// (non-Windows non-Linux, and Linux with the `linux-capture` feature off).
/// Returns `None` so the scan/repair routes report "engine unavailable"
/// rather than serving an empty capture, exactly as on an unsupported host.
#[cfg(not(any(windows, all(target_os = "linux", feature = "linux-capture"))))]
mod platform {
    pub fn capture_bgra(_x: i64, _y: i64, _w: i64, _h: i64) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_positive_region_never_captures() {
        // The dimension guard short-circuits before any GDI call (and the
        // off-Windows stub returns None regardless), so both capture forms
        // refuse a zero or negative region on every host.
        assert!(capture_region_png(10, 10, 0, 5).is_none());
        assert!(capture_region_png(10, 10, 5, 0).is_none());
        assert!(capture_region_png(10, 10, -1, 5).is_none());
        assert!(capture_region_bgr(10, 10, 0, 5).is_none());
        assert!(capture_region_bgr(10, 10, 5, -3).is_none());
    }
}
