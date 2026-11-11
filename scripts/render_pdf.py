"""Render a JSON array of HTML documents to sequential PDF files."""

import json
from pathlib import Path
import resource
import sys


UNAVAILABLE = "RECIPE_SHELF_PDF_UNAVAILABLE"
FAILED = "RECIPE_SHELF_PDF_FAILED"
MAX_ADDRESS_SPACE = 1024 * 1024 * 1024
MAX_PDF_BYTES = 128 * 1024 * 1024


def set_resource_limits() -> None:
    limits = (
        (resource.RLIMIT_CPU, 120),
        (resource.RLIMIT_AS, MAX_ADDRESS_SPACE),
        (resource.RLIMIT_FSIZE, MAX_PDF_BYTES),
    )
    for limit_kind, requested in limits:
        try:
            _soft, hard = resource.getrlimit(limit_kind)
            cap = requested if hard == resource.RLIM_INFINITY else min(requested, hard)
            resource.setrlimit(limit_kind, (cap, cap))
        except (OSError, ValueError):
            # The parent process still enforces its wall-clock and output limits.
            continue


def limit_next_output(remaining_bytes: int) -> None:
    _soft, hard = resource.getrlimit(resource.RLIMIT_FSIZE)
    cap = remaining_bytes if hard == resource.RLIM_INFINITY else min(remaining_bytes, hard)
    resource.setrlimit(resource.RLIMIT_FSIZE, (cap, hard))


def deny_url_fetcher(_url):
    raise ValueError("External resource loading is disabled")


def main() -> int:
    if len(sys.argv) != 3:
        print(FAILED, file=sys.stderr)
        return 2
    input_path, output_dir = Path(sys.argv[1]), Path(sys.argv[2])
    try:
        from weasyprint import HTML
    except (ImportError, OSError):
        print(UNAVAILABLE, file=sys.stderr)
        return 3

    try:
        documents = json.loads(input_path.read_text(encoding="utf-8"))
        if not isinstance(documents, list) or not all(isinstance(item, str) for item in documents):
            raise ValueError("Expected a list of HTML strings")
        output_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        set_resource_limits()
        total_pdf_bytes = 0
        for index, document in enumerate(documents):
            remaining = MAX_PDF_BYTES - total_pdf_bytes
            if remaining <= 0:
                raise ValueError("PDF output limit exceeded")
            limit_next_output(remaining)
            output_path = output_dir / f"{index}.pdf"
            HTML(string=document, url_fetcher=deny_url_fetcher).write_pdf(
                target=str(output_path)
            )
            total_pdf_bytes += output_path.stat().st_size
            if total_pdf_bytes > MAX_PDF_BYTES:
                raise ValueError("PDF output limit exceeded")
    except Exception:
        # Keep renderer diagnostics and potentially sensitive template text out
        # of application logs and user-facing errors.
        print(FAILED, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
