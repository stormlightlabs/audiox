#!/usr/bin/env python3
"""Convert a VS Code color theme JSON file to tmTheme (TextMate plist XML)."""

import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from xml.dom import minidom


def build_dict_element(parent: ET.Element, entries: dict[str, str]) -> ET.Element:
    d = ET.SubElement(parent, "dict")
    for key, value in entries.items():
        ET.SubElement(d, "key").text = key
        ET.SubElement(d, "string").text = value
    return d


def convert(theme: dict) -> str:
    plist = ET.Element("plist", version="1.0")
    root = ET.SubElement(plist, "dict")

    # Name
    ET.SubElement(root, "key").text = "name"
    ET.SubElement(root, "string").text = theme.get(
        "displayName", theme.get("name", "Converted Theme")
    )

    # Settings array
    ET.SubElement(root, "key").text = "settings"
    settings_array = ET.SubElement(root, "array")

    # First entry: global settings derived from editor.* colors
    colors = theme.get("colors", {})
    global_settings: dict[str, str] = {}
    color_map = {
        "editor.background": "background",
        "editor.foreground": "foreground",
        "editor.selectionBackground": "selection",
        "editor.lineHighlightBackground": "lineHighlight",
        "editorCursor.foreground": "caret",
        "editorWhitespace.foreground": "invisibles",
        "editor.findMatchHighlightBackground": "findHighlight",
        "editor.findMatchBackground": "findHighlightForeground",
        "editorBracketMatch.border": "bracketsForeground",
        "editorGroup.border": "guide",
        "editorIndentGuide.activeBackground1": "activeGuide",
        "editorIndentGuide.background1": "stackGuide",
    }
    for json_key, tm_key in color_map.items():
        if json_key in colors and colors[json_key] not in ("#0000", "#00000000"):
            global_settings[tm_key] = colors[json_key]

    global_dict = ET.SubElement(settings_array, "dict")
    ET.SubElement(global_dict, "key").text = "settings"
    build_dict_element(global_dict, global_settings)

    # Token colors
    for token in theme.get("tokenColors", []):
        entry = ET.SubElement(settings_array, "dict")

        # Name (optional)
        name = token.get("name")
        if name:
            ET.SubElement(entry, "key").text = "name"
            ET.SubElement(entry, "string").text = name

        # Scope
        scope = token.get("scope", [])
        if isinstance(scope, list):
            scope = ", ".join(scope)
        if scope:
            ET.SubElement(entry, "key").text = "scope"
            ET.SubElement(entry, "string").text = scope

        # Settings
        ts = token.get("settings", {})
        if ts:
            ET.SubElement(entry, "key").text = "settings"
            build_dict_element(entry, {k: v for k, v in ts.items() if v})

    raw_xml = ET.tostring(plist, encoding="unicode")
    dom = minidom.parseString(raw_xml)

    pretty = dom.toprettyxml(indent="  ", encoding=None)
    # Strip the default xml declaration minidom adds, we'll prepend our own with DOCTYPE
    lines = [
        line
        for line in pretty.splitlines()
        if line.strip() and not line.startswith("<?xml")
    ]
    body = "\n".join(lines)

    header = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"'
        ' "http://www.apple.com/DTDs/PropertyList-1.0.dtd">'
    )
    return f"{header}\n{body}"


def main() -> None:
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <theme.json> [output.tmTheme]", file=sys.stderr)
        sys.exit(1)

    input_path = Path(sys.argv[1])
    if len(sys.argv) >= 3:
        output_path = Path(sys.argv[2])
    else:
        output_path = input_path.with_suffix(".tmTheme")

    with open(input_path, encoding="utf-8") as f:
        theme = json.load(f)

    result = convert(theme)

    with open(output_path, "w", encoding="utf-8") as f:
        f.write(result)
        f.write("\n")

    print(f"Wrote {output_path}")


if __name__ == "__main__":
    main()
