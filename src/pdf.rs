//! Isolated HTML-to-PDF rendering through the system WeasyPrint package.

use crate::exports::ExportError;
use anyhow::{Result, anyhow, bail};
use serde_json::to_writer;
use std::{
    fs::{self, File},
    io::{Read, Write},
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use tempfile::tempdir;

const MAX_HTML_BYTES: usize = 64 * 1024 * 1024;
const MAX_PDF_BYTES: u64 = 128 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 4096;
const RENDER_TIMEOUT: Duration = Duration::from_secs(120);
const RENDER_SCRIPT: &str = include_str!("../scripts/render_pdf.py");
static RENDER_LOCK: Mutex<()> = Mutex::new(());

/// Render HTML documents in one bounded WeasyPrint process, preserving order.
///
/// The renderer receives only a private JSON input file and private output
/// directory. No caller-provided paths are passed to Python.
pub fn render_documents(html: &[String]) -> Result<Vec<Vec<u8>>> {
    if html.is_empty() {
        return Ok(Vec::new());
    }
    let total_html = html
        .iter()
        .try_fold(0usize, |total, document| total.checked_add(document.len()))
        .ok_or_else(|| anyhow!("PDF HTML input exceeds the configured limit"))?;
    if total_html > MAX_HTML_BYTES {
        bail!("PDF HTML input exceeds the configured limit")
    }
    let _render_guard = RENDER_LOCK
        .lock()
        .map_err(|_| anyhow!("PDF renderer lock is unavailable"))?;

    let workspace = tempdir().map_err(|_| anyhow!("could not create private PDF workspace"))?;
    let input_path = workspace.path().join("input.json");
    let output_dir = workspace.path().join("output");
    fs::create_dir(&output_dir)
        .map_err(|_| anyhow!("could not create private PDF output directory"))?;
    let mut input = File::create(&input_path)
        .map_err(|_| anyhow!("could not create private PDF renderer input"))?;
    to_writer(&mut input, html).map_err(|_| anyhow!("could not encode PDF renderer input"))?;
    input
        .flush()
        .map_err(|_| anyhow!("could not write PDF renderer input"))?;

    let python = std::env::var_os("PDF_PYTHON").unwrap_or_else(|| "python3".into());
    let mut child = Command::new(python)
        .arg("-c")
        .arg(RENDER_SCRIPT)
        .arg(&input_path)
        .arg(&output_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ExportError::PdfUnavailable)?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("PDF renderer stderr unavailable"))?;
    let stderr_reader = thread::spawn(move || bounded_drain(stderr, MAX_STDERR_BYTES));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < RENDER_TIMEOUT => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_reader.join();
                bail!("PDF rendering timed out")
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_reader.join();
                bail!("PDF renderer process failed")
            }
        }
    };
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow!("PDF renderer process failed"))
        .unwrap_or_default();
    if !status.success() {
        if stderr
            .windows(b"RECIPE_SHELF_PDF_UNAVAILABLE".len())
            .any(|chunk| chunk == b"RECIPE_SHELF_PDF_UNAVAILABLE")
        {
            return Err(ExportError::PdfUnavailable.into());
        }
        bail!("PDF rendering failed")
    }

    let mut result = Vec::with_capacity(html.len());
    let mut total_pdf_bytes = 0u64;
    for index in 0..html.len() {
        let path = output_dir.join(format!("{index}.pdf"));
        let length = fs::metadata(&path)
            .map_err(|_| anyhow!("PDF renderer did not produce all requested documents"))?
            .len();
        total_pdf_bytes = total_pdf_bytes
            .checked_add(length)
            .ok_or_else(|| anyhow!("PDF output exceeds the configured limit"))?;
        if length == 0 || length > MAX_PDF_BYTES || total_pdf_bytes > MAX_PDF_BYTES {
            bail!("PDF output exceeds the configured limit")
        }
        let bytes = fs::read(path).map_err(|_| anyhow!("could not read private PDF output"))?;
        if !bytes.starts_with(b"%PDF-") {
            bail!("PDF renderer returned invalid output")
        }
        result.push(bytes);
    }
    Ok(result)
}

fn bounded_drain(mut stderr: impl Read, max_bytes: usize) -> Vec<u8> {
    let mut captured = Vec::with_capacity(max_bytes);
    let mut buffer = [0u8; 2048];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let remaining = max_bytes.saturating_sub(captured.len());
                captured.extend_from_slice(&buffer[..count.min(remaining)]);
            }
        }
    }
    captured
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn weasyprint_available() -> bool {
        Command::new(std::env::var_os("PDF_PYTHON").unwrap_or_else(|| "python3".into()))
            .args(["-c", "import weasyprint"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[test]
    fn rejects_oversized_html_before_launching_python() {
        let oversized = vec!["x".repeat(MAX_HTML_BYTES + 1)];
        assert!(render_documents(&oversized).is_err());
    }

    #[test]
    fn renders_unicode_multipage_batch_and_denies_local_resources() {
        if !weasyprint_available() {
            if std::env::var_os("REQUIRE_PDF_TESTS").is_some() {
                panic!("REQUIRE_PDF_TESTS is set but WeasyPrint is unavailable");
            }
            eprintln!("SKIP: WeasyPrint unavailable; PDF renderer integration not run");
            return;
        }
        let directory = tempdir().unwrap();
        let external_css = directory.path().join("external.css");
        fs::write(
            &external_css,
            "p::before { content: 'LOCAL_SECRET_NEVER_LOAD'; }",
        )
        .unwrap();
        let local_url = format!("file://{}", external_css.display());
        let long_body = (0..180)
            .map(|n| format!("<p>Recipe step {n}: stir and cook until tender.</p>"))
            .collect::<String>();
        let html = vec![
            "<!doctype html><meta charset=utf-8><p>Crème brûlée</p>".to_owned(),
            format!(
                "<!doctype html><meta charset=utf-8><style>@page {{ size: letter; margin: 0.5in; }}</style>{long_body}"
            ),
            format!(
                "<!doctype html><meta charset=utf-8><link rel=stylesheet href=\"{local_url}\"><p>Safe recipe</p>"
            ),
        ];
        let pdfs = render_documents(&html).unwrap();
        assert_eq!(pdfs.len(), 3);
        assert!(pdfs.iter().all(|pdf| pdf.starts_with(b"%PDF-")));

        let first = directory.path().join("first.pdf");
        let long = directory.path().join("long.pdf");
        let denied = directory.path().join("denied.pdf");
        fs::write(&first, &pdfs[0]).unwrap();
        fs::write(&long, &pdfs[1]).unwrap();
        fs::write(&denied, &pdfs[2]).unwrap();
        if let Ok(info) = Command::new("pdfinfo").arg(&first).output()
            && info.status.success()
        {
            let pages = String::from_utf8_lossy(&info.stdout)
                .lines()
                .find_map(|line| line.strip_prefix("Pages:"))
                .and_then(|value| value.trim().parse::<usize>().ok());
            assert_eq!(pages, Some(1));
            let info = Command::new("pdfinfo").arg(&long).output().unwrap();
            let pages = String::from_utf8_lossy(&info.stdout)
                .lines()
                .find_map(|line| line.strip_prefix("Pages:"))
                .and_then(|value| value.trim().parse::<usize>().ok());
            assert!(pages.is_some_and(|count| count > 1));
        }
        if let Ok(text) = Command::new("pdftotext").arg(&first).arg("-").output()
            && text.status.success()
        {
            assert!(String::from_utf8_lossy(&text.stdout).contains("Crème brûlée"));
            let text = Command::new("pdftotext")
                .arg(&denied)
                .arg("-")
                .output()
                .unwrap();
            assert!(text.status.success());
            assert!(!String::from_utf8_lossy(&text.stdout).contains("LOCAL_SECRET_NEVER_LOAD"));
        }
    }
}
