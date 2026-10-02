//! User-owned PNG packs, decoded once by AppKit. Manifest coordinates are top-left.
use crate::model::Activity;
use objc2::{AnyThread, rc::Retained};
use objc2_app_kit::{
    NSBitmapImageRep, NSColorSpace, NSCompositingOperation, NSGraphicsContext, NSImage,
    NSImageInterpolation,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FrameSpec {
    File(String),
    Crop {
        file: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        #[serde(default)]
        duration_ms: Option<u64>,
    },
}
impl FrameSpec {
    fn file(&self) -> &str {
        match self {
            Self::File(s) => s,
            Self::Crop { file, .. } => file,
        }
    }
}
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: u32,
    #[serde(default = "default_interval")]
    pub frame_ms: u64,
    pub idle: Vec<FrameSpec>,
    #[serde(default)]
    pub pixel_art: bool,
    #[serde(default)]
    pub celebrate: Vec<FrameSpec>,
    #[serde(default)]
    pub sleep: Vec<FrameSpec>,
    #[serde(default)]
    pub working: Vec<FrameSpec>,
    #[serde(default)]
    pub waiting: Vec<FrameSpec>,
    #[serde(default)]
    pub compacting: Vec<FrameSpec>,
    #[serde(default)]
    pub error: Vec<FrameSpec>,
    #[serde(default)]
    pub disconnected: Vec<FrameSpec>,
}
fn default_interval() -> u64 {
    200
}
impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Sprite manifest version must be 1".into());
        }
        if self.idle.is_empty() {
            return Err("At least one idle frame is required".into());
        }
        if !(80..=2000).contains(&self.frame_ms) {
            return Err("frame_ms must be between 80 and 2000".into());
        }
        let all = [
            &self.idle,
            &self.working,
            &self.waiting,
            &self.compacting,
            &self.error,
            &self.disconnected,
            &self.celebrate,
            &self.sleep,
        ];
        if all.iter().map(|v| v.len()).sum::<usize>() > 128 {
            return Err("At most 128 frames per pack".into());
        }
        for frame in all.into_iter().flatten() {
            if Path::new(frame.file()).is_absolute()
                || !frame.file().to_lowercase().ends_with(".png")
            {
                return Err("Frames must reference relative PNG paths".into());
            }
            if let FrameSpec::Crop {
                duration_ms: Some(ms),
                ..
            } = frame
            {
                if !(80..=2000).contains(ms) {
                    return Err("duration_ms must be between 80 and 2000".into());
                }
            }
            if let FrameSpec::Crop {
                x,
                y,
                width,
                height,
                ..
            } = frame
            {
                if ![x, y, width, height].into_iter().all(|v| v.is_finite())
                    || *x < 0.
                    || *y < 0.
                    || *width <= 0.
                    || *height <= 0.
                {
                    return Err("Invalid sprite-sheet crop".into());
                }
            }
        }
        Ok(())
    }
}
// Alpha data is top-down; AppKit source rectangles are bottom-up.
fn opaque_bounds(w: usize, h: usize, alpha: &[u8], source: NSRect) -> Option<NSRect> {
    let mut left = usize::MAX;
    let mut bottom = usize::MAX;
    let mut right = 0;
    let mut top = 0;
    let ox = source.origin.x as usize;
    let oy = source.origin.y as usize;
    for y in 0..source.size.height as usize {
        for x in 0..source.size.width as usize {
            if ox + x < w && oy + y < h && alpha[(h - 1 - oy - y) * w + ox + x] >= 16 {
                left = left.min(x);
                bottom = bottom.min(y);
                right = right.max(x + 1);
                top = top.max(y + 1);
            }
        }
    }
    (left != usize::MAX).then(|| {
        NSRect::new(
            NSPoint::new(left as f64, bottom as f64),
            NSSize::new((right - left) as f64, (top - bottom) as f64),
        )
    })
}
struct Frame {
    image: Retained<NSImage>,
    source: NSRect,
    duration_ms: u64,
}
pub struct SpritePack {
    states: BTreeMap<&'static str, Vec<Frame>>,
    pub path: PathBuf,
    pub accent: [u8; 3],
    pixel_art: bool,
}
impl SpritePack {
    pub fn load(path: &Path, _mtm: MainThreadMarker) -> Result<Self, String> {
        let root = path.canonicalize().map_err(|e| e.to_string())?;
        let bytes = fs::read(root.join("manifest.json")).map_err(|e| e.to_string())?;
        if bytes.len() > 65536 {
            return Err("Sprite manifest is too large".into());
        }
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        manifest.validate()?;
        let mut images = BTreeMap::<PathBuf, Retained<NSImage>>::new();
        let mut alpha_images = BTreeMap::<PathBuf, (usize, usize, Vec<u8>)>::new();
        let mut pixels = 0u64;
        let mut encoded = 0u64;
        let mut states = BTreeMap::new();
        let mut palette = crate::palette::Palette::default();
        for (key, specs) in [
            ("idle", manifest.idle),
            ("working", manifest.working),
            ("waiting", manifest.waiting),
            ("compacting", manifest.compacting),
            ("error", manifest.error),
            ("disconnected", manifest.disconnected),
            ("celebrate", manifest.celebrate),
            ("sleep", manifest.sleep),
        ] {
            let mut frames = Vec::new();
            for spec in specs {
                let file = root
                    .join(spec.file())
                    .canonicalize()
                    .map_err(|e| e.to_string())?;
                if !file.starts_with(&root) {
                    return Err("Sprite paths must stay inside the pack".into());
                }
                if !images.contains_key(&file) {
                    encoded += fs::metadata(&file).map_err(|e| e.to_string())?.len();
                    if encoded > 16 * 1024 * 1024 {
                        return Err("PNG files exceed 16 MiB".into());
                    }
                    let image = NSImage::initWithContentsOfFile(
                        NSImage::alloc(),
                        &NSString::from_str(&file.to_string_lossy()),
                    )
                    .ok_or_else(|| format!("Could not load {}", file.display()))?;
                    if !image.isValid() {
                        return Err("Invalid PNG image".into());
                    }
                    for rep in image.representations() {
                        let w = rep.pixelsWide();
                        let h = rep.pixelsHigh();
                        if w <= 0 || h <= 0 || w > 2048 || h > 2048 {
                            return Err("PNG dimensions must be 1–2048 pixels".into());
                        }
                        pixels += (w as u64) * (h as u64);
                    }
                    if pixels > 4 * 1024 * 1024 {
                        return Err("Pack exceeds 4 million decoded pixels".into());
                    }
                    let space = NSColorSpace::sRGBColorSpace();
                    for rep in image.representations() {
                        if let Some(bitmap) = rep.downcast_ref::<NSBitmapImageRep>() {
                            let w = bitmap.pixelsWide() as usize;
                            let h = bitmap.pixelsHigh() as usize;
                            image.setSize(NSSize::new(w as f64, h as f64));
                            let mut alpha = vec![0; w * h];
                            for y in 0..bitmap.pixelsHigh() {
                                for x in 0..bitmap.pixelsWide() {
                                    if let Some(c) = bitmap
                                        .colorAtX_y(x, y)
                                        .and_then(|c| c.colorUsingColorSpace(&space))
                                    {
                                        alpha[y as usize * w + x as usize] =
                                            (c.alphaComponent() * 255.).round() as u8;
                                        palette.add(
                                            [
                                                (c.redComponent() * 255.).round() as u8,
                                                (c.greenComponent() * 255.).round() as u8,
                                                (c.blueComponent() * 255.).round() as u8,
                                            ],
                                            (c.alphaComponent() * 255.).round() as u8,
                                        );
                                    }
                                }
                            }
                            alpha_images.insert(file.clone(), (w, h, alpha));
                            break;
                        }
                    }
                    images.insert(file.clone(), image);
                }
                let image = images[&file].clone();
                let size = image.size();
                let duration_ms = match &spec {
                    FrameSpec::Crop {
                        duration_ms: Some(ms),
                        ..
                    } => *ms,
                    _ => manifest.frame_ms,
                };
                let source = match spec {
                    FrameSpec::File(_) => NSRect::new(NSPoint::new(0., 0.), size),
                    FrameSpec::Crop {
                        x,
                        y,
                        width,
                        height,
                        ..
                    } => {
                        if x + width > size.width || y + height > size.height {
                            return Err("Crop is outside the image".into());
                        }
                        NSRect::new(
                            NSPoint::new(x, size.height - y - height),
                            NSSize::new(width, height),
                        )
                    }
                };
                let (w, h, alpha) = alpha_images
                    .get(&file)
                    .ok_or("PNG has no bitmap representation")?;
                let visible = opaque_bounds(*w, *h, alpha, source)
                    .ok_or("Sprite frame is fully transparent")?;
                frames.push(Frame {
                    image,
                    source: NSRect::new(
                        NSPoint::new(
                            source.origin.x + visible.origin.x,
                            source.origin.y + visible.origin.y,
                        ),
                        visible.size,
                    ),
                    duration_ms,
                });
            }
            states.insert(key, frames);
        }
        Ok(Self {
            states,
            path: root,
            accent: palette.dominant(),
            pixel_art: manifest.pixel_art,
        })
    }
    fn frames(&self, activity: Activity, sleeping: bool) -> &[Frame] {
        if sleeping {
            return if self.states["sleep"].is_empty() {
                &self.states["idle"][..1]
            } else {
                &self.states["sleep"]
            };
        }
        let key = match activity {
            Activity::Idle => "idle",
            Activity::Working => "working",
            Activity::Waiting => "waiting",
            Activity::Compacting => "compacting",
            Activity::Error => "error",
            Activity::Disconnected => "disconnected",
        };
        let frames = &self.states[key];
        if frames.is_empty() {
            &self.states["idle"]
        } else {
            frames
        }
    }
    pub fn animated(&self, activity: Activity, sleeping: bool) -> bool {
        self.frames(activity, sleeping).len() > 1 && (!sleeping || self.states["idle"].len() > 1)
    }
    pub fn has_celebration(&self) -> bool {
        !self.states["celebrate"].is_empty()
    }
    pub fn duration(
        &self,
        activity: Activity,
        index: usize,
        celebrate: bool,
        sleeping: bool,
    ) -> u64 {
        let frames = if !sleeping && celebrate && self.has_celebration() {
            &self.states["celebrate"]
        } else {
            self.frames(activity, sleeping)
        };
        let duration = frames[index % frames.len()].duration_ms;
        if sleeping {
            // Never introduce a faster timer than the pack's slowest idle frame.
            duration.max(crate::sleep::MIN_SLEEP_FRAME_MS).max(
                self.states["idle"]
                    .iter()
                    .map(|f| f.duration_ms)
                    .max()
                    .unwrap_or(1000),
            )
        } else {
            duration
        }
    }
    pub fn celebration_frames(&self) -> usize {
        self.states["celebrate"].len()
    }
    pub fn draw(
        &self,
        activity: Activity,
        index: usize,
        celebrate: bool,
        sleeping: bool,
        bounds: NSRect,
    ) {
        let frames = if !sleeping && celebrate && self.has_celebration() {
            &self.states["celebrate"]
        } else {
            self.frames(activity, sleeping)
        };
        let frame = &frames[index % frames.len()];
        let scale = (bounds.size.width / frame.source.size.width)
            .min(bounds.size.height / frame.source.size.height);
        let size = NSSize::new(
            frame.source.size.width * scale,
            frame.source.size.height * scale,
        );
        let dst = NSRect::new(
            NSPoint::new(
                bounds.origin.x + (bounds.size.width - size.width) / 2.,
                bounds.origin.y + (bounds.size.height - size.height) / 2.,
            ),
            size,
        );
        let context = NSGraphicsContext::currentContext();
        let old = context.as_ref().map(|c| c.imageInterpolation());
        if self.pixel_art {
            if let Some(c) = &context {
                c.setImageInterpolation(NSImageInterpolation::None);
            }
        }
        frame.image.drawInRect_fromRect_operation_fraction(
            dst,
            frame.source,
            NSCompositingOperation::SourceOver,
            1.,
        );
        if let (Some(context), Some(old)) = (context, old) {
            context.setImageInterpolation(old);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transparent_padding_does_not_determine_sprite_scale() {
        let mut alpha = vec![0u8; 24 * 80];
        for y in 8..32 {
            for x in 1..23 {
                alpha[y * 24 + x] = 255;
            }
        }
        let bounds = opaque_bounds(
            24,
            80,
            &alpha,
            NSRect::new(NSPoint::new(0., 0.), NSSize::new(24., 80.)),
        )
        .unwrap();
        assert_eq!(bounds.size, NSSize::new(22., 24.));
        assert_eq!(bounds.origin.y, 48.);
    }
    #[test]
    fn validates_sheet_crops_and_requires_an_idle_fallback() {
        let mut m:Manifest=serde_json::from_str(r#"{"version":1,"idle":["idle.png"],"working":[{"file":"sheet.png","x":0,"y":0,"width":64,"height":64}]}"#).unwrap();
        assert!(m.validate().is_ok());
        m.working = vec![FrameSpec::Crop {
            file: "sheet.png".into(),
            x: 0.,
            y: -1.,
            width: 64.,
            height: 64.,
            duration_ms: None,
        }];
        assert!(m.validate().is_err());
        m.working.clear();
        m.idle.clear();
        assert!(m.validate().is_err());
    }
}
