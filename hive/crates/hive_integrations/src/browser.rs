//! Playwright-powered browser automation via CLI.
//!
//! Drives Playwright through generated Node.js scripts executed via
//! `tokio::process::Command`. Each method builds a self-contained script that
//! imports Playwright, launches a browser, performs the requested actions,
//! emits JSON to stdout, and exits. The JSON is captured and deserialized on
//! the Rust side.
//!
//! This approach avoids a native Playwright Rust crate dependency while
//! providing full programmatic browser control: headless rendering, form
//! filling, network interception, accessibility auditing, performance
//! metrics, site crawling, and more.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

// ── Browser type ────────────────────────────────────────────────────

/// Playwright browser engine to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserType {
    Chromium,
    Firefox,
    #[serde(rename = "webkit")]
    WebKit,
}

impl fmt::Display for BrowserType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chromium => write!(f, "chromium"),
            Self::Firefox => write!(f, "firefox"),
            Self::WebKit => write!(f, "webkit"),
        }
    }
}

impl Default for BrowserType {
    fn default() -> Self {
        Self::Chromium
    }
}

// ── Data types ──────────────────────────────────────────────────────

/// Basic information about a navigated page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    pub status_code: u16,
}

/// A hyperlink extracted from a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub text: String,
    pub href: String,
    pub is_external: bool,
}

/// Rich content extracted from a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageContent {
    pub url: String,
    pub title: String,
    pub text_content: String,
    pub links: Vec<Link>,
    pub meta_tags: HashMap<String, String>,
}

/// A single form field to fill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub selector: String,
    pub value: String,
}

/// The result of submitting a form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormResult {
    pub success: bool,
    pub submitted_url: String,
    pub response_status: u16,
}

/// Options for taking a screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotOptions {
    /// Capture the full scrollable page rather than just the viewport.
    #[serde(default)]
    pub full_page: bool,
    /// Viewport width in pixels.
    #[serde(default = "default_viewport_width")]
    pub width: u32,
    /// Viewport height in pixels.
    #[serde(default = "default_viewport_height")]
    pub height: u32,
    /// Optional CSS selector to screenshot a specific element.
    pub selector: Option<String>,
    /// Image format: "png" or "jpeg".
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_viewport_width() -> u32 {
    1280
}
fn default_viewport_height() -> u32 {
    720
}
fn default_format() -> String {
    "png".to_string()
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            full_page: false,
            width: default_viewport_width(),
            height: default_viewport_height(),
            selector: None,
            format: default_format(),
        }
    }
}

/// A page discovered during a crawl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub links: Vec<String>,
    pub depth: u32,
}

/// A detected content change on a monitored page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub timestamp: DateTime<Utc>,
    pub old_content: String,
    pub new_content: String,
    pub selector: String,
}

/// A captured network request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRequest {
    pub url: String,
    pub method: String,
    pub status: u16,
    pub content_type: String,
    pub body_size: u64,
}

/// An accessibility violation found during an audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A11yViolation {
    pub id: String,
    pub description: String,
    pub impact: String,
    pub nodes: Vec<String>,
}

/// Summary of an accessibility audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityReport {
    pub violations: Vec<A11yViolation>,
    pub passes: usize,
    pub total: usize,
}

/// Core Web Vitals and performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub first_contentful_paint_ms: f64,
    pub largest_contentful_paint_ms: f64,
    pub time_to_interactive_ms: f64,
    pub total_blocking_time_ms: f64,
    pub cumulative_layout_shift: f64,
}

/// Result of running a Playwright test script.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub passed: usize,
    pub failed: usize,
    pub duration_ms: u64,
    pub output: String,
}

/// A named CSS selector with an optional attribute to extract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeSelector {
    pub name: String,
    pub css_selector: String,
    pub attribute: Option<String>,
}

// ── Browser actions (script generation) ─────────────────────────────

/// An individual action to embed inside a generated Node.js script.
#[derive(Debug, Clone)]
pub enum BrowserAction {
    Navigate {
        url: String,
    },
    Screenshot {
        options: ScreenshotOptions,
    },
    GetContent,
    FillForm {
        fields: Vec<FormField>,
    },
    Click {
        selector: String,
    },
    EvaluateScript {
        code: String,
    },
    WaitForSelector {
        selector: String,
        timeout_ms: u64,
    },
    ScrapeStructured {
        selectors: Vec<ScrapeSelector>,
    },
    PdfExport,
    InterceptNetwork {
        url_pattern: String,
    },
    AccessibilityAudit,
    PerformanceMetrics,
}

// ── BrowserAutomation ───────────────────────────────────────────────

/// Playwright-powered browser automation client.
///
/// Generates and executes Node.js scripts that drive Playwright to perform
/// browser actions. Results are returned as structured Rust types parsed
/// from the script's JSON stdout.
pub struct BrowserAutomation {
    /// Optional explicit path to a `playwright` binary. When `None` the
    /// system `npx playwright` / global `node` is used.
    playwright_path: Option<String>,
    /// Run the browser in headless mode (default `true`).
    headless: bool,
    /// Which browser engine to use.
    browser_type: BrowserType,
    /// Global timeout in milliseconds for page operations.
    timeout_ms: u64,
}

impl BrowserAutomation {
    /// Create a new `BrowserAutomation` with sensible defaults.
    ///
    /// Defaults: headless mode, Chromium engine, 30-second timeout.
    pub fn new() -> Self {
        debug!("creating BrowserAutomation with default settings");
        Self {
            playwright_path: None,
            headless: true,
            browser_type: BrowserType::default(),
            timeout_ms: 30_000,
        }
    }

    /// Set the path to a local Playwright installation.
    pub fn with_playwright_path(mut self, path: impl Into<String>) -> Self {
        self.playwright_path = Some(path.into());
        self
    }

    /// Control whether the browser is launched in headless mode.
    pub fn with_headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// Choose the browser engine (Chromium, Firefox, or WebKit).
    pub fn with_browser_type(mut self, browser_type: BrowserType) -> Self {
        self.browser_type = browser_type;
        self
    }

    /// Set the global timeout for page operations.
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Return the configured browser type.
    pub fn browser_type(&self) -> BrowserType {
        self.browser_type
    }

    /// Return whether headless mode is enabled.
    pub fn headless(&self) -> bool {
        self.headless
    }

    /// Return the configured timeout in milliseconds.
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    // ── Installation ────────────────────────────────────────────────

    /// Ensure Playwright and the selected browser engine are installed.
    ///
    /// Runs `npx playwright install <browser>` which downloads the engine
    /// binaries if they are not already present.
    pub async fn ensure_installed(&self) -> Result<()> {
        let engine = self.browser_type.to_string();
        debug!(engine = %engine, "ensuring Playwright browser is installed");

        let output = tokio::process::Command::new("npx")
            .args(["playwright", "install", &engine])
            .output()
            .await
            .context("failed to run `npx playwright install`")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "Playwright install failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            );
        }

        debug!("Playwright browser engine installed successfully");
        Ok(())
    }

    // ── High-level API ──────────────────────────────────────────────

    /// Navigate to a URL and return basic page information.
    pub async fn navigate(&self, url: &str) -> Result<PageInfo> {
        debug!(url = %url, "navigating to URL");

        let script = self.generate_script(&[BrowserAction::Navigate {
            url: url.to_string(),
        }]);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result).context("failed to parse PageInfo from script output")
    }

    /// Take a screenshot of a URL and return the image bytes.
    pub async fn screenshot(&self, url: &str, options: ScreenshotOptions) -> Result<Vec<u8>> {
        debug!(url = %url, full_page = options.full_page, "taking screenshot");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::Screenshot { options },
        ]);
        let result = self.execute_script(&script).await?;

        let b64 = result["data"]
            .as_str()
            .context("screenshot script did not return base64 data")?;

        base64_decode(b64).context("failed to decode screenshot base64 data")
    }

    /// Extract page content: title, full text, links, and meta tags.
    pub async fn get_page_content(&self, url: &str) -> Result<PageContent> {
        debug!(url = %url, "extracting page content");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::GetContent,
        ]);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result).context("failed to parse PageContent from script output")
    }

    /// Fill form fields on a page and submit.
    pub async fn fill_form(&self, url: &str, fields: Vec<FormField>) -> Result<FormResult> {
        debug!(url = %url, fields = fields.len(), "filling form");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::FillForm { fields },
        ]);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result).context("failed to parse FormResult from script output")
    }

    /// Click an element on a page.
    pub async fn click(&self, url: &str, selector: &str) -> Result<()> {
        debug!(url = %url, selector = %selector, "clicking element");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::Click {
                selector: selector.to_string(),
            },
        ]);
        self.execute_script(&script).await?;
        Ok(())
    }

    /// Evaluate arbitrary JavaScript in the page context.
    pub async fn evaluate_script(
        &self,
        url: &str,
        js_code: &str,
    ) -> Result<serde_json::Value> {
        debug!(url = %url, "evaluating script in page context");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::EvaluateScript {
                code: js_code.to_string(),
            },
        ]);
        self.execute_script(&script).await
    }

    /// Wait for a CSS selector to appear on the page.
    pub async fn wait_for_selector(
        &self,
        url: &str,
        selector: &str,
        timeout_ms: u64,
    ) -> Result<bool> {
        debug!(url = %url, selector = %selector, timeout_ms = timeout_ms, "waiting for selector");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::WaitForSelector {
                selector: selector.to_string(),
                timeout_ms,
            },
        ]);
        let result = self.execute_script(&script).await?;

        result["found"]
            .as_bool()
            .context("wait_for_selector script did not return `found` boolean")
    }

    /// Scrape structured data from a page using named CSS selectors.
    pub async fn scrape_structured(
        &self,
        url: &str,
        selectors: Vec<ScrapeSelector>,
    ) -> Result<HashMap<String, Vec<String>>> {
        debug!(url = %url, selectors = selectors.len(), "scraping structured data");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::ScrapeStructured { selectors },
        ]);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result)
            .context("failed to parse structured scrape results from script output")
    }

    /// Export a page as a PDF and return the raw bytes.
    pub async fn pdf_export(&self, url: &str) -> Result<Vec<u8>> {
        debug!(url = %url, "exporting page as PDF");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::PdfExport,
        ]);
        let result = self.execute_script(&script).await?;

        let b64 = result["data"]
            .as_str()
            .context("PDF export script did not return base64 data")?;

        base64_decode(b64).context("failed to decode PDF base64 data")
    }

    /// Run a raw Playwright test script and return the results.
    pub async fn run_test(&self, test_script: &str) -> Result<TestResult> {
        debug!("running Playwright test script");

        let script = self.wrap_test_script(test_script);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result).context("failed to parse TestResult from script output")
    }

    /// Crawl a site starting from `base_url`, visiting up to `max_pages`
    /// pages and extracting content matched by `extract_selector`.
    pub async fn crawl_site(
        &self,
        base_url: &str,
        max_pages: usize,
        extract_selector: Option<&str>,
    ) -> Result<Vec<CrawledPage>> {
        debug!(
            base_url = %base_url,
            max_pages = max_pages,
            "starting site crawl"
        );

        let script = self.generate_crawl_script(base_url, max_pages, extract_selector);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result).context("failed to parse crawl results from script output")
    }

    /// Monitor a page element for changes at the given interval.
    ///
    /// Returns a channel receiver that yields [`ChangeEvent`]s whenever
    /// the content of the selected element changes. The monitoring task
    /// runs in the background until the receiver is dropped.
    pub async fn monitor_changes(
        &self,
        url: &str,
        selector: &str,
        interval_secs: u64,
    ) -> Result<mpsc::Receiver<ChangeEvent>> {
        debug!(
            url = %url,
            selector = %selector,
            interval_secs = interval_secs,
            "starting change monitor"
        );

        let (tx, rx) = mpsc::channel(64);
        let url = url.to_string();
        let selector = selector.to_string();
        let automation = Self {
            playwright_path: self.playwright_path.clone(),
            headless: self.headless,
            browser_type: self.browser_type,
            timeout_ms: self.timeout_ms,
        };

        tokio::spawn(async move {
            let mut previous_content: Option<String> = None;

            loop {
                let script = automation.generate_monitor_script(&url, &selector);
                match automation.execute_script(&script).await {
                    Ok(result) => {
                        if let Some(content) = result["content"].as_str() {
                            let content = content.to_string();
                            if let Some(ref prev) = previous_content {
                                if *prev != content {
                                    let event = ChangeEvent {
                                        timestamp: Utc::now(),
                                        old_content: prev.clone(),
                                        new_content: content.clone(),
                                        selector: selector.clone(),
                                    };
                                    if tx.send(event).await.is_err() {
                                        debug!("monitor receiver dropped, stopping");
                                        break;
                                    }
                                }
                            }
                            previous_content = Some(content);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "monitor poll failed");
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
            }
        });

        Ok(rx)
    }

    /// Intercept network requests matching a URL pattern.
    pub async fn intercept_network(
        &self,
        url: &str,
        url_pattern: &str,
    ) -> Result<Vec<NetworkRequest>> {
        debug!(url = %url, pattern = %url_pattern, "intercepting network requests");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::InterceptNetwork {
                url_pattern: url_pattern.to_string(),
            },
        ]);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result)
            .context("failed to parse network requests from script output")
    }

    /// Run an accessibility audit on a page.
    pub async fn accessibility_audit(&self, url: &str) -> Result<AccessibilityReport> {
        debug!(url = %url, "running accessibility audit");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::AccessibilityAudit,
        ]);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result)
            .context("failed to parse accessibility report from script output")
    }

    /// Collect performance metrics for a page.
    pub async fn performance_metrics(&self, url: &str) -> Result<PerformanceMetrics> {
        debug!(url = %url, "collecting performance metrics");

        let script = self.generate_script(&[
            BrowserAction::Navigate {
                url: url.to_string(),
            },
            BrowserAction::PerformanceMetrics,
        ]);
        let result = self.execute_script(&script).await?;

        serde_json::from_value(result)
            .context("failed to parse performance metrics from script output")
    }

    // ── Script generation ───────────────────────────────────────────

    /// Build a complete, self-contained Node.js script from a sequence
    /// of [`BrowserAction`]s.
    ///
    /// The script:
    /// 1. Imports the Playwright browser engine.
    /// 2. Launches the browser (headless or headed).
    /// 3. Opens a new page with the configured viewport and timeout.
    /// 4. Executes each action sequentially.
    /// 5. Prints a JSON result to stdout.
    /// 6. Closes the browser.
    fn generate_script(&self, actions: &[BrowserAction]) -> String {
        let browser_type = self.browser_type.to_string();
        let headless = self.headless;
        let timeout = self.timeout_ms;

        let mut lines = Vec::with_capacity(64);

        // ── Preamble ────────────────────────────────────────────────
        lines.push(format!(
            "const {{ {browser_type} }} = require('playwright');"
        ));
        lines.push(String::new());
        lines.push("(async () => {".to_string());
        lines.push("  let browser;".to_string());
        lines.push("  try {".to_string());
        lines.push(format!(
            "    browser = await {browser_type}.launch({{ headless: {headless} }});"
        ));
        lines.push("    const context = await browser.newContext();".to_string());
        lines.push(format!(
            "    context.setDefaultTimeout({timeout});"
        ));
        lines.push("    const page = await context.newPage();".to_string());
        lines.push("    let _result = {};".to_string());
        lines.push(String::new());

        // ── Actions ─────────────────────────────────────────────────
        for action in actions {
            match action {
                BrowserAction::Navigate { url } => {
                    let escaped = escape_js_string(url);
                    lines.push(format!(
                        "    const response = await page.goto('{escaped}', {{ waitUntil: 'domcontentloaded' }});"
                    ));
                    lines.push(
                        "    _result = { url: page.url(), title: await page.title(), status_code: response ? response.status() : 0 };"
                            .to_string(),
                    );
                }

                BrowserAction::Screenshot { options } => {
                    lines.push(format!(
                        "    await page.setViewportSize({{ width: {}, height: {} }});",
                        options.width, options.height
                    ));

                    let full_page = options.full_page;
                    let format = escape_js_string(&options.format);

                    if let Some(ref sel) = options.selector {
                        let sel_escaped = escape_js_string(sel);
                        lines.push(format!(
                            "    const element = await page.locator('{sel_escaped}').first();"
                        ));
                        lines.push(format!(
                            "    const screenshotBuf = await element.screenshot({{ type: '{format}' }});"
                        ));
                    } else {
                        lines.push(format!(
                            "    const screenshotBuf = await page.screenshot({{ fullPage: {full_page}, type: '{format}' }});"
                        ));
                    }

                    lines.push(
                        "    _result = { data: screenshotBuf.toString('base64') };".to_string(),
                    );
                }

                BrowserAction::GetContent => {
                    lines.push("    const bodyText = await page.evaluate(() => document.body.innerText);".to_string());
                    lines.push("    const links = await page.evaluate(() => {".to_string());
                    lines.push("      return Array.from(document.querySelectorAll('a[href]')).map(a => ({".to_string());
                    lines.push("        text: a.innerText.trim().substring(0, 200),".to_string());
                    lines.push("        href: a.href,".to_string());
                    lines.push("        is_external: a.hostname !== location.hostname".to_string());
                    lines.push("      }));".to_string());
                    lines.push("    });".to_string());
                    lines.push("    const metaTags = await page.evaluate(() => {".to_string());
                    lines.push("      const meta = {};".to_string());
                    lines.push("      document.querySelectorAll('meta[name], meta[property]').forEach(el => {".to_string());
                    lines.push("        const key = el.getAttribute('name') || el.getAttribute('property');".to_string());
                    lines.push("        if (key) meta[key] = el.getAttribute('content') || '';".to_string());
                    lines.push("      });".to_string());
                    lines.push("      return meta;".to_string());
                    lines.push("    });".to_string());
                    lines.push(
                        "    _result = { url: page.url(), title: await page.title(), text_content: bodyText, links, meta_tags: metaTags };"
                            .to_string(),
                    );
                }

                BrowserAction::FillForm { fields } => {
                    for field in fields {
                        let sel = escape_js_string(&field.selector);
                        let val = escape_js_string(&field.value);
                        lines.push(format!(
                            "    await page.locator('{sel}').fill('{val}');"
                        ));
                    }
                    lines.push(
                        "    await page.locator('form').first().evaluate(form => form.submit());".to_string(),
                    );
                    lines.push(
                        "    await page.waitForLoadState('domcontentloaded');".to_string(),
                    );
                    lines.push(
                        "    _result = { success: true, submitted_url: page.url(), response_status: 200 };"
                            .to_string(),
                    );
                }

                BrowserAction::Click { selector } => {
                    let sel = escape_js_string(selector);
                    lines.push(format!(
                        "    await page.locator('{sel}').first().click();"
                    ));
                    lines.push(
                        "    _result = { clicked: true };".to_string(),
                    );
                }

                BrowserAction::EvaluateScript { code } => {
                    let escaped = escape_js_string(code);
                    lines.push(format!(
                        "    _result = await page.evaluate(() => {{ {escaped} }});"
                    ));
                }

                BrowserAction::WaitForSelector {
                    selector,
                    timeout_ms,
                } => {
                    let sel = escape_js_string(selector);
                    lines.push("    let found = false;".to_string());
                    lines.push("    try {".to_string());
                    lines.push(format!(
                        "      await page.locator('{sel}').first().waitFor({{ timeout: {timeout_ms} }});"
                    ));
                    lines.push("      found = true;".to_string());
                    lines.push("    } catch (_) {}".to_string());
                    lines.push("    _result = { found };".to_string());
                }

                BrowserAction::ScrapeStructured { selectors } => {
                    lines.push("    const scraped = {};".to_string());
                    for sel in selectors {
                        let name = escape_js_string(&sel.name);
                        let css = escape_js_string(&sel.css_selector);
                        if let Some(ref attr) = sel.attribute {
                            let attr_escaped = escape_js_string(attr);
                            lines.push(format!(
                                "    scraped['{name}'] = await page.locator('{css}').evaluateAll((els) => els.map(el => el.getAttribute('{attr_escaped}') || ''));"
                            ));
                        } else {
                            lines.push(format!(
                                "    scraped['{name}'] = await page.locator('{css}').evaluateAll((els) => els.map(el => el.innerText.trim()));"
                            ));
                        }
                    }
                    lines.push("    _result = scraped;".to_string());
                }

                BrowserAction::PdfExport => {
                    lines.push(
                        "    const pdfBuf = await page.pdf({ format: 'A4', printBackground: true });"
                            .to_string(),
                    );
                    lines.push(
                        "    _result = { data: pdfBuf.toString('base64') };".to_string(),
                    );
                }

                BrowserAction::InterceptNetwork { url_pattern } => {
                    let pat = escape_js_string(url_pattern);
                    // Rewrite: navigate with interception enabled
                    lines.push("    const captured = [];".to_string());
                    lines.push("    page.on('response', async (resp) => {".to_string());
                    lines.push(format!(
                        "      if (resp.url().includes('{pat}')) {{"
                    ));
                    lines.push("        captured.push({".to_string());
                    lines.push("          url: resp.url(),".to_string());
                    lines.push(
                        "          method: resp.request().method(),".to_string(),
                    );
                    lines.push("          status: resp.status(),".to_string());
                    lines.push(
                        "          content_type: resp.headers()['content-type'] || '',".to_string(),
                    );
                    lines.push(
                        "          body_size: (await resp.body().catch(() => Buffer.alloc(0))).length"
                            .to_string(),
                    );
                    lines.push("        });".to_string());
                    lines.push("      }".to_string());
                    lines.push("    });".to_string());
                    lines.push(
                        "    await page.waitForTimeout(5000);".to_string(),
                    );
                    lines.push("    _result = captured;".to_string());
                }

                BrowserAction::AccessibilityAudit => {
                    lines.push("    const snapshot = await page.accessibility.snapshot();".to_string());
                    lines.push("    const violations = [];".to_string());
                    lines.push("    let passes = 0;".to_string());
                    lines.push("    let total = 0;".to_string());
                    lines.push("    function walk(node, depth) {".to_string());
                    lines.push("      if (!node) return;".to_string());
                    lines.push("      total++;".to_string());
                    lines.push("      const issues = [];".to_string());
                    lines.push("      if (node.role === 'img' && !node.name) {".to_string());
                    lines.push("        issues.push({ id: 'image-alt', description: 'Image missing alt text', impact: 'critical', nodes: [node.role + (node.name ? ': ' + node.name : '')] });".to_string());
                    lines.push("      }".to_string());
                    lines.push("      if (node.role === 'link' && !node.name) {".to_string());
                    lines.push("        issues.push({ id: 'link-name', description: 'Link has no accessible name', impact: 'serious', nodes: [node.role] });".to_string());
                    lines.push("      }".to_string());
                    lines.push("      if (node.role === 'button' && !node.name) {".to_string());
                    lines.push("        issues.push({ id: 'button-name', description: 'Button has no accessible name', impact: 'critical', nodes: [node.role] });".to_string());
                    lines.push("      }".to_string());
                    lines.push("      if (issues.length > 0) { violations.push(...issues); } else { passes++; }".to_string());
                    lines.push("      if (node.children) { node.children.forEach(c => walk(c, depth + 1)); }".to_string());
                    lines.push("    }".to_string());
                    lines.push("    walk(snapshot, 0);".to_string());
                    lines.push(
                        "    _result = { violations, passes, total };".to_string(),
                    );
                }

                BrowserAction::PerformanceMetrics => {
                    lines.push("    await page.waitForLoadState('networkidle');".to_string());
                    lines.push("    const perfData = await page.evaluate(() => {".to_string());
                    lines.push("      const perf = performance.getEntriesByType('navigation')[0] || {};".to_string());
                    lines.push("      const paint = performance.getEntriesByType('paint');".to_string());
                    lines.push("      const fcp = paint.find(e => e.name === 'first-contentful-paint');".to_string());
                    lines.push("      return {".to_string());
                    lines.push("        first_contentful_paint_ms: fcp ? fcp.startTime : 0,".to_string());
                    lines.push("        largest_contentful_paint_ms: 0,".to_string());
                    lines.push("        time_to_interactive_ms: perf.domInteractive ? perf.domInteractive - perf.fetchStart : 0,".to_string());
                    lines.push("        total_blocking_time_ms: 0,".to_string());
                    lines.push("        cumulative_layout_shift: 0".to_string());
                    lines.push("      };".to_string());
                    lines.push("    });".to_string());
                    // Attempt to get LCP and CLS through PerformanceObserver
                    lines.push("    const lcpAndCls = await page.evaluate(() => {".to_string());
                    lines.push("      return new Promise((resolve) => {".to_string());
                    lines.push("        let lcp = 0, cls = 0;".to_string());
                    lines.push("        try {".to_string());
                    lines.push("          new PerformanceObserver((list) => {".to_string());
                    lines.push("            const entries = list.getEntries();".to_string());
                    lines.push("            if (entries.length > 0) lcp = entries[entries.length - 1].startTime;".to_string());
                    lines.push("          }).observe({ type: 'largest-contentful-paint', buffered: true });".to_string());
                    lines.push("          new PerformanceObserver((list) => {".to_string());
                    lines.push("            for (const entry of list.getEntries()) {".to_string());
                    lines.push("              if (!entry.hadRecentInput) cls += entry.value;".to_string());
                    lines.push("            }".to_string());
                    lines.push("          }).observe({ type: 'layout-shift', buffered: true });".to_string());
                    lines.push("        } catch (_) {}".to_string());
                    lines.push("        setTimeout(() => resolve({ lcp, cls }), 1000);".to_string());
                    lines.push("      });".to_string());
                    lines.push("    });".to_string());
                    lines.push("    perfData.largest_contentful_paint_ms = lcpAndCls.lcp || perfData.largest_contentful_paint_ms;".to_string());
                    lines.push("    perfData.cumulative_layout_shift = lcpAndCls.cls || perfData.cumulative_layout_shift;".to_string());
                    lines.push("    _result = perfData;".to_string());
                }
            }
        }

        // ── Epilogue ────────────────────────────────────────────────
        lines.push(String::new());
        lines.push("    console.log(JSON.stringify(_result));".to_string());
        lines.push("  } catch (err) {".to_string());
        lines.push(
            "    console.error(JSON.stringify({ error: err.message, stack: err.stack }));"
                .to_string(),
        );
        lines.push("    process.exit(1);".to_string());
        lines.push("  } finally {".to_string());
        lines.push("    if (browser) await browser.close();".to_string());
        lines.push("  }".to_string());
        lines.push("})();".to_string());

        lines.join("\n")
    }

    /// Generate a Node.js script that crawls a site.
    fn generate_crawl_script(
        &self,
        base_url: &str,
        max_pages: usize,
        extract_selector: Option<&str>,
    ) -> String {
        let browser_type = self.browser_type.to_string();
        let headless = self.headless;
        let timeout = self.timeout_ms;
        let url_escaped = escape_js_string(base_url);
        let selector_js = match extract_selector {
            Some(sel) => format!("'{}'", escape_js_string(sel)),
            None => "null".to_string(),
        };

        format!(
            r#"const {{ {browser_type} }} = require('playwright');

(async () => {{
  let browser;
  try {{
    browser = await {browser_type}.launch({{ headless: {headless} }});
    const context = await browser.newContext();
    context.setDefaultTimeout({timeout});

    const baseUrl = '{url_escaped}';
    const baseHost = new URL(baseUrl).hostname;
    const maxPages = {max_pages};
    const extractSelector = {selector_js};
    const visited = new Set();
    const queue = [{{ url: baseUrl, depth: 0 }}];
    const results = [];

    while (queue.length > 0 && results.length < maxPages) {{
      const {{ url, depth }} = queue.shift();
      if (visited.has(url)) continue;
      visited.add(url);

      const page = await context.newPage();
      try {{
        const resp = await page.goto(url, {{ waitUntil: 'domcontentloaded', timeout: {timeout} }});
        if (!resp || resp.status() >= 400) {{ await page.close(); continue; }}

        const title = await page.title();
        let content = '';
        if (extractSelector) {{
          content = await page.locator(extractSelector).evaluateAll(els => els.map(el => el.innerText.trim()).join('\n')).catch(() => '');
        }} else {{
          content = await page.evaluate(() => document.body.innerText).catch(() => '');
        }}

        const links = await page.evaluate((host) => {{
          return Array.from(document.querySelectorAll('a[href]'))
            .map(a => a.href)
            .filter(h => {{
              try {{ return new URL(h).hostname === host; }} catch {{ return false; }}
            }});
        }}, baseHost);

        results.push({{ url, title, content: content.substring(0, 5000), links, depth }});

        for (const link of links) {{
          if (!visited.has(link) && results.length + queue.length < maxPages) {{
            queue.push({{ url: link, depth: depth + 1 }});
          }}
        }}
      }} catch (e) {{
        // Skip pages that error
      }} finally {{
        await page.close();
      }}
    }}

    console.log(JSON.stringify(results));
  }} catch (err) {{
    console.error(JSON.stringify({{ error: err.message, stack: err.stack }}));
    process.exit(1);
  }} finally {{
    if (browser) await browser.close();
  }}
}})();"#
        )
    }

    /// Generate a script that fetches the text content of a single
    /// selector (used by the change monitor).
    fn generate_monitor_script(&self, url: &str, selector: &str) -> String {
        let browser_type = self.browser_type.to_string();
        let headless = self.headless;
        let timeout = self.timeout_ms;
        let url_escaped = escape_js_string(url);
        let sel_escaped = escape_js_string(selector);

        format!(
            r#"const {{ {browser_type} }} = require('playwright');

(async () => {{
  let browser;
  try {{
    browser = await {browser_type}.launch({{ headless: {headless} }});
    const context = await browser.newContext();
    context.setDefaultTimeout({timeout});
    const page = await context.newPage();
    await page.goto('{url_escaped}', {{ waitUntil: 'domcontentloaded' }});
    const content = await page.locator('{sel_escaped}').first().innerText();
    console.log(JSON.stringify({{ content }}));
  }} catch (err) {{
    console.error(JSON.stringify({{ error: err.message }}));
    process.exit(1);
  }} finally {{
    if (browser) await browser.close();
  }}
}})();"#
        )
    }

    /// Wrap a user-provided test script in a harness that captures pass/
    /// fail counts and outputs JSON.
    fn wrap_test_script(&self, test_script: &str) -> String {
        let browser_type = self.browser_type.to_string();
        let headless = self.headless;
        let timeout = self.timeout_ms;
        let escaped_test = escape_js_string(test_script);

        format!(
            r#"const {{ {browser_type} }} = require('playwright');

(async () => {{
  let browser;
  const start = Date.now();
  let passed = 0;
  let failed = 0;
  const output = [];

  function assert(condition, message) {{
    if (condition) {{
      passed++;
      output.push('PASS: ' + (message || 'assertion'));
    }} else {{
      failed++;
      output.push('FAIL: ' + (message || 'assertion'));
    }}
  }}

  try {{
    browser = await {browser_type}.launch({{ headless: {headless} }});
    const context = await browser.newContext();
    context.setDefaultTimeout({timeout});
    const page = await context.newPage();

    {escaped_test}

    const duration_ms = Date.now() - start;
    console.log(JSON.stringify({{ passed, failed, duration_ms, output: output.join('\n') }}));
  }} catch (err) {{
    const duration_ms = Date.now() - start;
    failed++;
    output.push('ERROR: ' + err.message);
    console.log(JSON.stringify({{ passed, failed, duration_ms, output: output.join('\n') }}));
  }} finally {{
    if (browser) await browser.close();
  }}
}})();"#
        )
    }

    // ── Script execution ────────────────────────────────────────────

    /// Write a Node.js script to a temporary file, execute it with
    /// `node`, parse stdout as JSON, and clean up.
    async fn execute_script(&self, script: &str) -> Result<serde_json::Value> {
        let temp_dir = std::env::temp_dir();
        let script_id = uuid::Uuid::new_v4();
        let script_path = temp_dir.join(format!("hive_pw_{script_id}.mjs"));

        // Write script to temp file.
        tokio::fs::write(&script_path, script)
            .await
            .context("failed to write Playwright script to temp file")?;

        debug!(path = %script_path.display(), "executing Playwright script");

        let node_cmd = self.node_command();
        let timeout_duration = std::time::Duration::from_millis(self.timeout_ms + 10_000);

        let result = tokio::time::timeout(timeout_duration, async {
            let output = tokio::process::Command::new(&node_cmd)
                .arg(&script_path)
                .env("NODE_PATH", self.node_path())
                .output()
                .await
                .context("failed to execute Node.js process")?;

            Ok::<_, anyhow::Error>(output)
        })
        .await;

        // Always clean up the temp file.
        if let Err(e) = tokio::fs::remove_file(&script_path).await {
            warn!(
                path = %script_path.display(),
                error = %e,
                "failed to remove temp script file"
            );
        }

        let output = match result {
            Ok(inner) => inner?,
            Err(_) => bail!(
                "Playwright script timed out after {} ms",
                self.timeout_ms + 10_000
            ),
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Try to parse structured error from stderr.
            if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(stderr.trim()) {
                if let Some(msg) = err_json["error"].as_str() {
                    bail!("Playwright script error: {}", msg);
                }
            }
            bail!(
                "Playwright script failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stdout_trimmed = stdout.trim();

        if stdout_trimmed.is_empty() {
            bail!("Playwright script produced no output");
        }

        serde_json::from_str(stdout_trimmed)
            .context("failed to parse Playwright script JSON output")
    }

    /// Determine the `node` binary to use.
    fn node_command(&self) -> String {
        if let Some(ref pw_path) = self.playwright_path {
            // If a custom path is set, look for node alongside it.
            let parent = PathBuf::from(pw_path);
            if let Some(dir) = parent.parent() {
                let node = dir.join("node");
                if node.exists() {
                    return node.to_string_lossy().to_string();
                }
            }
        }
        "node".to_string()
    }

    /// Build the NODE_PATH so `require('playwright')` can find the
    /// package regardless of the working directory.
    fn node_path(&self) -> String {
        if let Some(ref pw_path) = self.playwright_path {
            let pw_dir = PathBuf::from(pw_path);
            if let Some(parent) = pw_dir.parent() {
                return parent.to_string_lossy().to_string();
            }
        }

        // Default: rely on npx / globally installed playwright.
        String::new()
    }
}

impl Default for BrowserAutomation {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for BrowserAutomation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserAutomation")
            .field("headless", &self.headless)
            .field("browser_type", &self.browser_type)
            .field("timeout_ms", &self.timeout_ms)
            .field(
                "playwright_path",
                &self.playwright_path.as_deref().unwrap_or("<npx>"),
            )
            .finish()
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Escape a string for safe embedding inside a JavaScript single-quoted
/// string literal.
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Decode a base64-encoded string into raw bytes.
///
/// Supports both standard and URL-safe base64, with or without padding.
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    // Strip whitespace that Node.js may have injected.
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();

    // Simple base64 decoder without pulling in an external crate.
    // Node outputs standard base64 with padding.
    let table: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(table: &[u8; 64], c: u8) -> Result<u8> {
        if c == b'+' || c == b'-' {
            return Ok(62);
        }
        if c == b'/' || c == b'_' {
            return Ok(63);
        }
        for (i, &t) in table.iter().enumerate() {
            if t == c {
                return Ok(i as u8);
            }
        }
        bail!("invalid base64 character: {}", c as char);
    }

    let bytes = cleaned.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);
    let chunks = bytes.chunks(4);

    for chunk in chunks {
        let len = chunk.iter().filter(|&&c| c != b'=').count();
        if len < 2 {
            continue;
        }

        let a = decode_char(table, chunk[0])?;
        let b = decode_char(table, chunk[1])?;
        result.push((a << 2) | (b >> 4));

        if len > 2 {
            let c = decode_char(table, chunk[2])?;
            result.push((b << 4) | (c >> 2));

            if len > 3 {
                let d = decode_char(table, chunk[3])?;
                result.push((c << 6) | d);
            }
        }
    }

    Ok(result)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── BrowserType ─────────────────────────────────────────────────

    #[test]
    fn test_browser_type_display() {
        assert_eq!(BrowserType::Chromium.to_string(), "chromium");
        assert_eq!(BrowserType::Firefox.to_string(), "firefox");
        assert_eq!(BrowserType::WebKit.to_string(), "webkit");
    }

    #[test]
    fn test_browser_type_default_is_chromium() {
        assert_eq!(BrowserType::default(), BrowserType::Chromium);
    }

    #[test]
    fn test_browser_type_serde_roundtrip() {
        for bt in [BrowserType::Chromium, BrowserType::Firefox, BrowserType::WebKit] {
            let json = serde_json::to_string(&bt).unwrap();
            let parsed: BrowserType = serde_json::from_str(&json).unwrap();
            assert_eq!(bt, parsed);
        }
    }

    #[test]
    fn test_browser_type_deserialize_strings() {
        let c: BrowserType = serde_json::from_str(r#""chromium""#).unwrap();
        assert_eq!(c, BrowserType::Chromium);

        let f: BrowserType = serde_json::from_str(r#""firefox""#).unwrap();
        assert_eq!(f, BrowserType::Firefox);

        let w: BrowserType = serde_json::from_str(r#""webkit""#).unwrap();
        assert_eq!(w, BrowserType::WebKit);
    }

    // ── BrowserAutomation construction ──────────────────────────────

    #[test]
    fn test_default_settings() {
        let ba = BrowserAutomation::new();
        assert!(ba.headless());
        assert_eq!(ba.browser_type(), BrowserType::Chromium);
        assert_eq!(ba.timeout_ms(), 30_000);
    }

    #[test]
    fn test_builder_methods() {
        let ba = BrowserAutomation::new()
            .with_headless(false)
            .with_browser_type(BrowserType::Firefox)
            .with_timeout_ms(60_000)
            .with_playwright_path("/usr/local/bin/playwright");

        assert!(!ba.headless());
        assert_eq!(ba.browser_type(), BrowserType::Firefox);
        assert_eq!(ba.timeout_ms(), 60_000);
    }

    #[test]
    fn test_debug_impl() {
        let ba = BrowserAutomation::new();
        let debug_str = format!("{:?}", ba);
        assert!(debug_str.contains("BrowserAutomation"));
        assert!(debug_str.contains("headless: true"));
        assert!(debug_str.contains("Chromium"));
    }

    #[test]
    fn test_default_trait() {
        let ba = BrowserAutomation::default();
        assert!(ba.headless());
        assert_eq!(ba.browser_type(), BrowserType::Chromium);
    }

    // ── Script generation ───────────────────────────────────────────

    #[test]
    fn test_generate_navigate_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        }]);

        assert!(script.contains("require('playwright')"));
        assert!(script.contains("chromium.launch"));
        assert!(script.contains("headless: true"));
        assert!(script.contains("page.goto('https://example.com'"));
        assert!(script.contains("JSON.stringify(_result)"));
        assert!(script.contains("browser.close()"));
    }

    #[test]
    fn test_generate_script_firefox() {
        let ba = BrowserAutomation::new().with_browser_type(BrowserType::Firefox);
        let script = ba.generate_script(&[BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        }]);

        assert!(script.contains("const { firefox } = require('playwright')"));
        assert!(script.contains("firefox.launch"));
    }

    #[test]
    fn test_generate_script_webkit_headed() {
        let ba = BrowserAutomation::new()
            .with_browser_type(BrowserType::WebKit)
            .with_headless(false);
        let script = ba.generate_script(&[BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        }]);

        assert!(script.contains("const { webkit } = require('playwright')"));
        assert!(script.contains("webkit.launch({ headless: false })"));
    }

    #[test]
    fn test_generate_screenshot_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::Screenshot {
                options: ScreenshotOptions {
                    full_page: true,
                    width: 1920,
                    height: 1080,
                    selector: None,
                    format: "png".to_string(),
                },
            },
        ]);

        assert!(script.contains("setViewportSize({ width: 1920, height: 1080 })"));
        assert!(script.contains("fullPage: true"));
        assert!(script.contains("toString('base64')"));
    }

    #[test]
    fn test_generate_screenshot_with_selector() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::Screenshot {
                options: ScreenshotOptions {
                    selector: Some("#hero".to_string()),
                    ..Default::default()
                },
            },
        ]);

        assert!(script.contains("page.locator('#hero')"));
        assert!(script.contains("element.screenshot"));
    }

    #[test]
    fn test_generate_content_extraction_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::GetContent,
        ]);

        assert!(script.contains("document.body.innerText"));
        assert!(script.contains("querySelectorAll('a[href]')"));
        assert!(script.contains("meta[name]"));
        assert!(script.contains("meta_tags"));
    }

    #[test]
    fn test_generate_fill_form_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com/form".to_string(),
            },
            BrowserAction::FillForm {
                fields: vec![
                    FormField {
                        selector: "#name".to_string(),
                        value: "Alice".to_string(),
                    },
                    FormField {
                        selector: "#email".to_string(),
                        value: "alice@example.com".to_string(),
                    },
                ],
            },
        ]);

        assert!(script.contains("locator('#name').fill('Alice')"));
        assert!(script.contains("locator('#email').fill('alice@example.com')"));
        assert!(script.contains("form.submit()"));
    }

    #[test]
    fn test_generate_click_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::Click {
                selector: "button.submit".to_string(),
            },
        ]);

        assert!(script.contains("locator('button.submit').first().click()"));
    }

    #[test]
    fn test_generate_evaluate_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::EvaluateScript {
                code: "return document.title".to_string(),
            },
        ]);

        assert!(script.contains("page.evaluate"));
        assert!(script.contains("return document.title"));
    }

    #[test]
    fn test_generate_wait_for_selector_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::WaitForSelector {
                selector: ".loaded".to_string(),
                timeout_ms: 5000,
            },
        ]);

        assert!(script.contains("locator('.loaded')"));
        assert!(script.contains("timeout: 5000"));
        assert!(script.contains("found"));
    }

    #[test]
    fn test_generate_scrape_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::ScrapeStructured {
                selectors: vec![
                    ScrapeSelector {
                        name: "titles".to_string(),
                        css_selector: "h2".to_string(),
                        attribute: None,
                    },
                    ScrapeSelector {
                        name: "links".to_string(),
                        css_selector: "a".to_string(),
                        attribute: Some("href".to_string()),
                    },
                ],
            },
        ]);

        assert!(script.contains("scraped['titles']"));
        assert!(script.contains("locator('h2')"));
        assert!(script.contains("scraped['links']"));
        assert!(script.contains("getAttribute('href')"));
    }

    #[test]
    fn test_generate_pdf_export_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::PdfExport,
        ]);

        assert!(script.contains("page.pdf"));
        assert!(script.contains("format: 'A4'"));
        assert!(script.contains("toString('base64')"));
    }

    #[test]
    fn test_generate_network_intercept_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::InterceptNetwork {
                url_pattern: "/api/".to_string(),
            },
        ]);

        assert!(script.contains("page.on('response'"));
        assert!(script.contains("includes('/api/')"));
        assert!(script.contains("captured"));
    }

    #[test]
    fn test_generate_accessibility_audit_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::AccessibilityAudit,
        ]);

        assert!(script.contains("page.accessibility.snapshot()"));
        assert!(script.contains("image-alt"));
        assert!(script.contains("link-name"));
        assert!(script.contains("button-name"));
        assert!(script.contains("violations"));
    }

    #[test]
    fn test_generate_performance_metrics_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::PerformanceMetrics,
        ]);

        assert!(script.contains("first-contentful-paint"));
        assert!(script.contains("largest-contentful-paint"));
        assert!(script.contains("layout-shift"));
        assert!(script.contains("domInteractive"));
    }

    #[test]
    fn test_generate_crawl_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_crawl_script("https://example.com", 10, Some("article"));

        assert!(script.contains("https://example.com"));
        assert!(script.contains("maxPages = 10"));
        assert!(script.contains("extractSelector = 'article'"));
        assert!(script.contains("queue"));
        assert!(script.contains("visited"));
    }

    #[test]
    fn test_generate_crawl_script_no_selector() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_crawl_script("https://example.com", 5, None);

        assert!(script.contains("extractSelector = null"));
        assert!(script.contains("document.body.innerText"));
    }

    #[test]
    fn test_generate_monitor_script() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_monitor_script("https://example.com", "#price");

        assert!(script.contains("https://example.com"));
        assert!(script.contains("#price"));
        assert!(script.contains("innerText"));
        assert!(script.contains("JSON.stringify({ content })"));
    }

    #[test]
    fn test_wrap_test_script() {
        let ba = BrowserAutomation::new();
        let script = ba.wrap_test_script("await page.goto('https://example.com');");

        assert!(script.contains("assert("));
        assert!(script.contains("passed"));
        assert!(script.contains("failed"));
        assert!(script.contains("duration_ms"));
        assert!(script.contains("page.goto"));
    }

    #[test]
    fn test_timeout_in_generated_script() {
        let ba = BrowserAutomation::new().with_timeout_ms(15_000);
        let script = ba.generate_script(&[BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        }]);

        assert!(script.contains("setDefaultTimeout(15000)"));
    }

    // ── JS string escaping ──────────────────────────────────────────

    #[test]
    fn test_escape_js_string_basic() {
        assert_eq!(escape_js_string("hello"), "hello");
    }

    #[test]
    fn test_escape_js_string_quotes() {
        assert_eq!(escape_js_string("it's"), "it\\'s");
    }

    #[test]
    fn test_escape_js_string_backslash() {
        assert_eq!(escape_js_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_escape_js_string_newlines() {
        assert_eq!(escape_js_string("a\nb\r\t"), "a\\nb\\r\\t");
    }

    #[test]
    fn test_escape_js_string_combined() {
        let input = "it's a\nnew \"day\"\\end";
        let escaped = escape_js_string(input);
        assert!(escaped.contains("\\'"));
        assert!(escaped.contains("\\n"));
        assert!(escaped.contains("\\\\"));
    }

    // ── Base64 decoding ─────────────────────────────────────────────

    #[test]
    fn test_base64_decode_simple() {
        // "SGVsbG8=" decodes to "Hello"
        let decoded = base64_decode("SGVsbG8=").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_base64_decode_no_padding() {
        // "SGVsbG8" (without =) should also decode to "Hello"
        let decoded = base64_decode("SGVsbG8").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_base64_decode_with_whitespace() {
        let decoded = base64_decode("  SGVs\nbG8=  ").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_base64_decode_empty() {
        let decoded = base64_decode("").unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_base64_decode_longer_text() {
        // "SGVsbG8gV29ybGQh" = "Hello World!"
        let decoded = base64_decode("SGVsbG8gV29ybGQh").unwrap();
        assert_eq!(decoded, b"Hello World!");
    }

    #[test]
    fn test_base64_decode_binary_chars() {
        // base64 for [0xFF, 0x00, 0xAB]
        // 0xFF = 11111111, 0x00 = 00000000, 0xAB = 10101011
        // b64: /wCr
        let decoded = base64_decode("/wCr").unwrap();
        assert_eq!(decoded, vec![0xFF, 0x00, 0xAB]);
    }

    // ── Data types serialization ────────────────────────────────────

    #[test]
    fn test_page_info_serde_roundtrip() {
        let info = PageInfo {
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
            status_code: 200,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: PageInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.url, info.url);
        assert_eq!(parsed.title, info.title);
        assert_eq!(parsed.status_code, 200);
    }

    #[test]
    fn test_link_serde_roundtrip() {
        let link = Link {
            text: "Click here".to_string(),
            href: "https://example.com/page".to_string(),
            is_external: true,
        };
        let json = serde_json::to_string(&link).unwrap();
        let parsed: Link = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, "Click here");
        assert!(parsed.is_external);
    }

    #[test]
    fn test_form_field_serde_roundtrip() {
        let field = FormField {
            selector: "#username".to_string(),
            value: "admin".to_string(),
        };
        let json = serde_json::to_string(&field).unwrap();
        let parsed: FormField = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.selector, "#username");
        assert_eq!(parsed.value, "admin");
    }

    #[test]
    fn test_form_result_serde_roundtrip() {
        let result = FormResult {
            success: true,
            submitted_url: "https://example.com/done".to_string(),
            response_status: 200,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: FormResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.response_status, 200);
    }

    #[test]
    fn test_screenshot_options_default() {
        let opts = ScreenshotOptions::default();
        assert!(!opts.full_page);
        assert_eq!(opts.width, 1280);
        assert_eq!(opts.height, 720);
        assert!(opts.selector.is_none());
        assert_eq!(opts.format, "png");
    }

    #[test]
    fn test_crawled_page_serde_roundtrip() {
        let page = CrawledPage {
            url: "https://example.com".to_string(),
            title: "Home".to_string(),
            content: "Welcome".to_string(),
            links: vec!["https://example.com/about".to_string()],
            depth: 0,
        };
        let json = serde_json::to_string(&page).unwrap();
        let parsed: CrawledPage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.depth, 0);
        assert_eq!(parsed.links.len(), 1);
    }

    #[test]
    fn test_network_request_serde_roundtrip() {
        let req = NetworkRequest {
            url: "https://api.example.com/data".to_string(),
            method: "GET".to_string(),
            status: 200,
            content_type: "application/json".to_string(),
            body_size: 1024,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: NetworkRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method, "GET");
        assert_eq!(parsed.body_size, 1024);
    }

    #[test]
    fn test_a11y_violation_serde_roundtrip() {
        let violation = A11yViolation {
            id: "image-alt".to_string(),
            description: "Image missing alt text".to_string(),
            impact: "critical".to_string(),
            nodes: vec!["img".to_string()],
        };
        let json = serde_json::to_string(&violation).unwrap();
        let parsed: A11yViolation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "image-alt");
        assert_eq!(parsed.impact, "critical");
    }

    #[test]
    fn test_accessibility_report_serde_roundtrip() {
        let report = AccessibilityReport {
            violations: vec![A11yViolation {
                id: "link-name".to_string(),
                description: "Link has no name".to_string(),
                impact: "serious".to_string(),
                nodes: vec!["a".to_string()],
            }],
            passes: 42,
            total: 43,
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: AccessibilityReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.violations.len(), 1);
        assert_eq!(parsed.passes, 42);
        assert_eq!(parsed.total, 43);
    }

    #[test]
    fn test_performance_metrics_serde_roundtrip() {
        let metrics = PerformanceMetrics {
            first_contentful_paint_ms: 250.0,
            largest_contentful_paint_ms: 800.0,
            time_to_interactive_ms: 1200.0,
            total_blocking_time_ms: 50.0,
            cumulative_layout_shift: 0.05,
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let parsed: PerformanceMetrics = serde_json::from_str(&json).unwrap();
        assert!((parsed.first_contentful_paint_ms - 250.0).abs() < f64::EPSILON);
        assert!((parsed.cumulative_layout_shift - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_test_result_serde_roundtrip() {
        let result = TestResult {
            passed: 5,
            failed: 1,
            duration_ms: 3200,
            output: "PASS: nav\nFAIL: button".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: TestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.passed, 5);
        assert_eq!(parsed.failed, 1);
        assert_eq!(parsed.duration_ms, 3200);
    }

    #[test]
    fn test_scrape_selector_serde_roundtrip() {
        let sel = ScrapeSelector {
            name: "prices".to_string(),
            css_selector: ".price".to_string(),
            attribute: Some("data-value".to_string()),
        };
        let json = serde_json::to_string(&sel).unwrap();
        let parsed: ScrapeSelector = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.attribute.as_deref(), Some("data-value"));
    }

    #[test]
    fn test_scrape_selector_no_attribute() {
        let sel = ScrapeSelector {
            name: "headings".to_string(),
            css_selector: "h1".to_string(),
            attribute: None,
        };
        let json = serde_json::to_string(&sel).unwrap();
        let parsed: ScrapeSelector = serde_json::from_str(&json).unwrap();
        assert!(parsed.attribute.is_none());
    }

    #[test]
    fn test_change_event_serde_roundtrip() {
        let event = ChangeEvent {
            timestamp: Utc::now(),
            old_content: "old".to_string(),
            new_content: "new".to_string(),
            selector: "#target".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: ChangeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.old_content, "old");
        assert_eq!(parsed.new_content, "new");
        assert_eq!(parsed.selector, "#target");
    }

    #[test]
    fn test_page_content_serde_roundtrip() {
        let content = PageContent {
            url: "https://example.com".to_string(),
            title: "Test".to_string(),
            text_content: "Hello world".to_string(),
            links: vec![Link {
                text: "About".to_string(),
                href: "/about".to_string(),
                is_external: false,
            }],
            meta_tags: {
                let mut m = HashMap::new();
                m.insert("description".to_string(), "A test page".to_string());
                m
            },
        };
        let json = serde_json::to_string(&content).unwrap();
        let parsed: PageContent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.title, "Test");
        assert_eq!(parsed.links.len(), 1);
        assert_eq!(
            parsed.meta_tags.get("description").map(|s| s.as_str()),
            Some("A test page")
        );
    }

    // ── Multiple actions composition ────────────────────────────────

    #[test]
    fn test_generate_multiple_actions() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[
            BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
            BrowserAction::Click {
                selector: "#login".to_string(),
            },
            BrowserAction::WaitForSelector {
                selector: "#dashboard".to_string(),
                timeout_ms: 10_000,
            },
        ]);

        // All actions should appear in order.
        let nav_pos = script.find("page.goto").unwrap();
        let click_pos = script.find("locator('#login')").unwrap();
        let wait_pos = script.find("locator('#dashboard')").unwrap();
        assert!(nav_pos < click_pos);
        assert!(click_pos < wait_pos);
    }

    #[test]
    fn test_script_has_error_handling() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        }]);

        assert!(script.contains("try {"));
        assert!(script.contains("} catch (err) {"));
        assert!(script.contains("} finally {"));
        assert!(script.contains("process.exit(1)"));
    }

    #[test]
    fn test_script_closes_browser_in_finally() {
        let ba = BrowserAutomation::new();
        let script = ba.generate_script(&[BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        }]);

        let finally_pos = script.find("} finally {").unwrap();
        let close_pos = script.find("await browser.close()").unwrap();
        assert!(close_pos > finally_pos);
    }

    // ── Node command resolution ─────────────────────────────────────

    #[test]
    fn test_node_command_default() {
        let ba = BrowserAutomation::new();
        assert_eq!(ba.node_command(), "node");
    }

    #[test]
    fn test_node_path_default_is_empty() {
        let ba = BrowserAutomation::new();
        assert_eq!(ba.node_path(), "");
    }

    #[test]
    fn test_node_path_with_playwright_path() {
        let ba = BrowserAutomation::new()
            .with_playwright_path("/opt/node_modules/.bin/playwright");
        let path = ba.node_path();
        assert_eq!(path, "/opt/node_modules/.bin");
    }
}
