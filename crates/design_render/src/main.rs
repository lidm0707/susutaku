//! design_render: paints every app page/feature state to PNG + element
//! bounding boxes, so layout positions can be checked against the design.
//!
//! Uses headless Chromium (Skia rasterization of the real pages) via CDP
//! instead of raw wgpu: drawing HTML with wgpu means re-implementing a
//! layout engine; wgpu is only the right tool later, for overlaying the
//! captured bounding boxes as position rects.

use anyhow::{bail, Result};
use headless_chrome::{Browser, LaunchOptionsBuilder, Tab};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

const MANIFEST: &str = include_str!("../pages.toml");
const OUT_DIR: &str = "bench/design";
const VIEWPORT: (u32, u32) = (1280, 720);
const SETTLE_MS: u64 = 700;

#[derive(Deserialize)]
struct Manifest {
    pages: Vec<PageSpec>,
}

#[derive(Deserialize)]
struct PageSpec {
    name: String,
    path: String,
    #[serde(default)]
    setup: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    click: String,
}

struct App {
    base_url: String,
    user: String,
    password: String,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn login(tab: &Tab, app: &App) -> Result<()> {
    tab.navigate_to(&format!("{}/", app.base_url))?;
    tab.wait_for_element("input[placeholder=\"username\"]")?;
    tab.find_element("input[placeholder=\"username\"]")?
        .type_into(&app.user)?;
    tab.find_element("input[placeholder=\"password\"]")?
        .type_into(&app.password)?;
    tab.find_element("button[type=\"submit\"]")?.click()?;
    tab.wait_for_element(".dock-nav")?;
    Ok(())
}

fn capture_boxes(tab: &Tab, out: &PathBuf) -> Result<usize> {
    let expr = r#"(() => {
        const sels = ['main', 'header', 'nav', 'form', '[role="dialog"]',
                      '[role="menu"]', 'button', 'input', 'select',
                      '.bubble', '.dock-nav', '.dock-host'];
        const seen = [];
        for (const sel of sels) {
            for (const el of document.querySelectorAll(sel)) {
                const r = el.getBoundingClientRect();
                if (!r.width && !r.height) continue;
                seen.push({ sel, x: r.x, y: r.y, w: r.width, h: r.height,
                            text: (el.textContent || '').trim().slice(0, 40) });
            }
        }
        return JSON.stringify(seen);
    })()"#;
    let eval = tab.evaluate(expr, false)?;
    let json = eval
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| "[]".into());
    let boxes: serde_json::Value = serde_json::from_str(&json)?;
    let n = boxes.as_array().map(|a| a.len()).unwrap_or(0);
    fs::write(out, serde_json::to_string_pretty(&boxes)?)?;
    Ok(n)
}

fn capture(tab: &Tab, spec: &PageSpec, app: &App) -> Result<()> {
    tab.navigate_to(&format!("{}{}", app.base_url, spec.path))?;
    tab.wait_for_element("body")?;
    std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));

    for step in &spec.setup {
        let el = tab.wait_for_element(&step.click)?;
        el.click()?;
        std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));
    }

    let png = PathBuf::from(OUT_DIR).join(format!("{}.png", spec.name));
    let bxs = PathBuf::from(OUT_DIR).join(format!("{}.boxes.json", spec.name));
    let shot = tab.capture_screenshot(
        headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
        None,
        None,
        true,
    )?;
    fs::write(&png, shot)?;
    let n = capture_boxes(tab, &bxs)?;
    println!("{n:>3} boxes  {}", png.display());
    Ok(())
}

fn main() -> Result<()> {
    let app = App {
        base_url: env_or("PLAYWRIGHT_BASE_URL", "http://localhost:3334"),
        user: env_or("E2E_USER", "owner"),
        password: env_or("E2E_PASSWORD", "walk-walk-1"),
    };
    fs::create_dir_all(OUT_DIR)?;

    let manifest: Manifest = toml::from_str(MANIFEST)?;
    let chrome_path = env_or("CHROME_PATH", "");
    let mut opts = LaunchOptionsBuilder::default();
    opts.headless(true).window_size(Some(VIEWPORT));
    if !chrome_path.is_empty() {
        opts.path(Some(PathBuf::from(chrome_path)));
    }
    let browser = Browser::new(opts.build().unwrap())?;
    let tab = browser.new_tab()?;

    match login(&tab, &app) {
        Ok(()) => println!("logged in as {}", app.user),
        Err(e) => bail!("login failed (is the stack up on {}?): {e}", app.base_url),
    }

    let mut ok = 0;
    for spec in &manifest.pages {
        match capture(&tab, spec, &app) {
            Ok(()) => ok += 1,
            Err(e) => eprintln!("SKIP {} : {e}", spec.name),
        }
    }
    println!("rendered {ok}/{} into {OUT_DIR}", manifest.pages.len());
    Ok(())
}
