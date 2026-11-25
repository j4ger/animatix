#!/usr/bin/env python3
"""List possible user-facing literals for review, with documented exclusions."""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates" / "animatix-gui" / "src"
WEB = ROOT / "web" / "app" / "src"

RUST_STRING = r'"(?P<copy>(?:\\.|[^"\\])+)"'
WIDGET_COPY = re.compile(
    r"(?:\.(?:button|small_button|label|link|heading|strong|small|weak|placeholder|hint_text|with_tooltip|on_hover_text|"
    r"set_title|set_status_info|set_status_error)|\b(?:RichText|Label|Alert|Toast|Button|CollapsingHeader)::"
    r"(?:new|primary|secondary|ghost|danger|warning|error|info|success))"
    r"\s*\(\s*(?:format!\(\s*)?" + RUST_STRING
)
STATUS_COPY = re.compile(
    r"(?:\b(?:preview\.status|status_bar)\s*=|\bEffect::Status\()\s*(?:format!\(\s*)?"
    + RUST_STRING
)
EXPORT_ROW = re.compile(r"\bSelf::settings_row\(\s*ui\s*,\s*" + RUST_STRING)
WEB_LITERAL_ATTRIBUTE = re.compile(
    r'(?:aria-label|title|placeholder)="(?P<copy>[^"]+)"'
)
WEB_SCRIPT_COPY = re.compile(
    r"""\b(?:textContent|innerText)\s*=\s*['"](?P<copy>[^'"\n]+)['"]"""
)

# UI surfaces can show authored code or ordinary property names by design.
# Record exclusions at the literal boundary instead of ignoring whole panels.
GUI_MESSAGE_IDS = set(re.findall(
    r"(?m)^([a-z][a-z0-9-]*)\s*=",
    (ROOT / "crates/animatix-i18n/locales/en/gui.ftl").read_text(),
))
GUI_EXCLUSIONS = {
    "/path/to/workspace": "filesystem placeholder",
    "/path/to/plugin.amx-plugin.toml": "filesystem placeholder",
    "@slots: {}": "AMX slot notation",
    "+0%": "accepted input syntax",
    "Ctrl+Enter": "keyboard notation",
    "Esc": "key name",
    "FFmpeg": "product name",
    "Python": "product name",
    "WebP": "format name",
    "WAV": "format name",
}
WEB_EXCLUSIONS = {
    "GitHub": "product name",
}


def rust_source(path):
    source = path.read_text()
    # Unit tests have literal expected English and AMX fixtures.
    source = re.split(r"(?m)^#\[cfg\(test\)\]\nmod tests\b", source, maxsplit=1)[0]
    return re.sub(r"(?m)^[ \t]*//.*$", "", source)


def report(path, source, expressions, exclusions):
    for expression in expressions:
        for match in expression.finditer(source):
            literal = match["copy"]
            if not re.search(r"[A-Za-z]{2}", literal) or literal in GUI_MESSAGE_IDS:
                continue
            line = source.count("\n", 0, match.start()) + 1
            disposition = "excluded: " + exclusions[literal] if literal in exclusions else "review"
            print(f"{disposition}\t{path.relative_to(ROOT)}:{line}\t{literal}")


def main():
    for path in sorted(GUI.rglob("*.rs")):
        relative = path.relative_to(GUI)
        if relative.parts[0] in {"bin", "dev"} or path.stem in {"tests", "row_tests", "test_support"}:
            continue
        report(path, rust_source(path), (WIDGET_COPY, STATUS_COPY, EXPORT_ROW), GUI_EXCLUSIONS)
    for path in sorted(WEB.rglob("*.astro")):
        report(path, path.read_text(), (WEB_LITERAL_ATTRIBUTE,), WEB_EXCLUSIONS)
    for path in sorted((WEB / "scripts").rglob("*.ts")):
        report(path, path.read_text(), (WEB_SCRIPT_COPY,), WEB_EXCLUSIONS)
    for path in sorted((ROOT / "web" / "embed" / "src").rglob("*.js")):
        report(path, path.read_text(), (WEB_SCRIPT_COPY,), WEB_EXCLUSIONS)


if __name__ == "__main__":
    main()
