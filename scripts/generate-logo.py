#!/usr/bin/env python3
"""
Generate pixel-art SVG logos spelling "Wickeban" with QR-code / glitch aesthetic.

Each character is defined on a pixel grid. Filled cells become solid rectangles.
Empty cells adjacent to filled cells get thin L-shaped finder bracket decorations,
creating a glitchy, encoded, data-matrix feel.

Outputs both a light (black fill) and dark (white fill) variant as single-path SVGs.
"""

import os

# ── Grid & cell dimensions ──────────────────────────────────────────────────
CELL_W = 7.2       # width of each pixel cell
CELL_H = 13.0      # height of each pixel cell
ROW_OVERLAP = 0.6   # each row starts 0.6 before prior row ends
BW = 0.86           # bracket width / height
# Bracket vertical offsets from row top
BRACKET_Y1 = 5.56   # upper bracket pair vertical position
BRACKET_Y2 = 7.44   # lower bracket pair vertical position
BRACKET_H = 0.86    # bracket bar thickness

CHAR_SPACING = 2    # empty columns between characters (in grid units)

# ── 5×7 pixel font definitions ──────────────────────────────────────────────
# Each character is a list of 7 strings; each string has 5 chars ('X' = filled, '.' = empty)
FONT = {
    'W': [
        "X...X",
        "X...X",
        "X...X",
        "X.X.X",
        "X.X.X",
        "XX.XX",
        ".X.X.",
    ],
    'i': [
        "..X..",
        ".....",
        "..X..",
        "..X..",
        "..X..",
        "..X..",
        "..X..",
    ],
    'c': [
        ".....",
        ".....",
        ".XXX.",
        "X....",
        "X....",
        "X....",
        ".XXX.",
    ],
    'k': [
        "X....",
        "X....",
        "X..X.",
        "X.X..",
        "XX...",
        "X.X..",
        "X..X.",
    ],
    'e': [
        ".....",
        ".....",
        ".XXX.",
        "X...X",
        "XXXXX",
        "X....",
        ".XXX.",
    ],
    'b': [
        "X....",
        "X....",
        "XXXX.",
        "X...X",
        "X...X",
        "X...X",
        "XXXX.",
    ],
    'a': [
        ".....",
        ".....",
        ".XXX.",
        "....X",
        ".XXXX",
        "X...X",
        ".XXXX",
    ],
    'n': [
        ".....",
        ".....",
        "X.XX.",
        "XX..X",
        "X...X",
        "X...X",
        "X...X",
    ],
}


def build_grid(text):
    """Build a 2D boolean grid for the given text, returns (grid, cols, rows)."""
    char_grids = []
    for ch in text:
        glyph = FONT[ch]
        char_grids.append(glyph)

    # Calculate total width
    total_cols = 0
    for i, glyph in enumerate(char_grids):
        total_cols += len(glyph[0])
        if i < len(char_grids) - 1:
            total_cols += CHAR_SPACING

    num_rows = 7  # all glyphs are 7 rows tall

    # Build grid
    grid = [[False] * total_cols for _ in range(num_rows)]

    col_offset = 0
    for gi, glyph in enumerate(char_grids):
        char_width = len(glyph[0])
        for row_idx, row_str in enumerate(glyph):
            for col_idx, pixel in enumerate(row_str):
                if pixel == 'X':
                    grid[row_idx][col_offset + col_idx] = True
        col_offset += char_width
        if gi < len(char_grids) - 1:
            col_offset += CHAR_SPACING

    return grid, total_cols, num_rows


def fmt(v):
    """Format a float, removing unnecessary trailing zeros."""
    s = f"{v:.4f}"
    # Remove trailing zeros after decimal point
    if '.' in s:
        s = s.rstrip('0').rstrip('.')
    # Special case: -0 → 0
    if s == '-0':
        s = '0'
    return s


def cell_xy(col, row):
    """Get the top-left (x, y) of a cell, accounting for row overlap."""
    x = col * CELL_W
    y = row * (CELL_H - ROW_OVERLAP)
    return x, y


def filled_rect_path(col, row):
    """Path for a filled (solid) cell rectangle."""
    x, y = cell_xy(col, row)
    w = CELL_W
    h = CELL_H
    # M(x, y+h) V(y) H(x+w) V(y+h) H(x) Z  — counterclockwise from bottom-left
    return f"M{fmt(x)} {fmt(y + h)}V{fmt(y)}H{fmt(x + w)}V{fmt(y + h)}H{fmt(x)}Z"


def left_bracket_pair(col, row):
    """
    L-bracket pair for an empty cell that has a filled neighbor to its LEFT.
    The brackets 'point' or open to the right (decorating the right edge of
    the filled region spilling into this empty cell).

    These are two thin L-shaped marks near the vertical center of the cell.
    """
    x, y = cell_xy(col, row)
    w = CELL_W
    h = CELL_H
    paths = []

    # Upper L-bracket: a thin bar at y + BRACKET_Y1 height, with a small vertical
    # tail going down from the left side
    # Shape: horizontal bar across cell width at bracket_y1, with thickness BW,
    # plus a vertical extension on the right side
    by1 = y + BRACKET_Y1
    by1_bot = by1 + BW

    # Upper bracket: thin horizontal bar from x to x+w at by1, height BW,
    # with vertical tail on right descending to y+h
    # M(x+w-BW, y+h) V(by1) H(x) V(by1-BW) ... no wait.

    # Based on the original sample paths for brackets on right side of filled region:
    # The bracket at right edge of filled → opening right into empty cell.
    # Re-examining the user's description more carefully:

    # "Empty cell to the RIGHT of filled" gets "L-bracket at RIGHT edge" → opening LEFT
    # "Empty cell to the LEFT of filled" gets "L-bracket at LEFT edge" → opening RIGHT

    # For empty cell with filled neighbor to LEFT (bracket opens right):
    # Upper bracket: L-shape with horizontal bar near top and vertical bar on left edge
    # Path: horizontal bar from (x) to (x + w) at bracket_y1 level,
    # with vertical piece on left from bracket_y1 down to y+h

    # Bracket pair 1 (upper):
    # An L with the corner at bottom-left, horizontal going right, vertical going up
    # Horizontal bar: from x to x+w at y_offset = BRACKET_Y1, thickness = BW
    # Vertical bar: from BRACKET_Y1 down to bottom of cell, thickness = BW, on left side
    p1 = (
        f"M{fmt(x)} {fmt(y + h)}"
        f"V{fmt(by1)}"
        f"H{fmt(x + w)}"
        f"V{fmt(by1_bot)}"
        f"H{fmt(x + BW)}"
        f"V{fmt(y + h)}"
        f"H{fmt(x)}Z"
    )
    paths.append(p1)

    # Bracket pair 2 (lower):
    by2 = y + BRACKET_Y2
    by2_bot = by2 + BW
    p2 = (
        f"M{fmt(x)} {fmt(y + h)}"
        f"V{fmt(by2)}"
        f"H{fmt(x + w)}"
        f"V{fmt(by2_bot)}"
        f"H{fmt(x + BW)}"
        f"V{fmt(y + h)}"
        f"H{fmt(x)}Z"
    )
    # Actually these two would overlap. Let me reconsider the structure.

    # Looking at the original sample more carefully:
    # The first bracket is at BRACKET_Y1 with corner at left, bar goes right
    # The second bracket is at BRACKET_Y2 with corner at left, shifted right by BW
    # They interlock like QR finder patterns

    # Let me re-derive from the user's sample paths:
    # Sample for "L-bracket at RIGHT edge" (empty cell to right of filled):
    # Bracket 1: M{x+w-bw} {y+h} V{y+5.56} H{x} V{y+4.70} H{x+w} V{y+h} H{x+w-bw} Z
    # Bracket 2: M{x+w-2*bw} {y+7.44} H{x} V{y+6.58} H{x+w-bw} V{y+h} H{x+w-2*bw} V{y+7.44} Z

    # So "RIGHT edge" bracket (empty cell has filled to LEFT):
    # Actually wait - "L-bracket at RIGHT edge of filled region (empty cell to the right)"
    # means the filled region's RIGHT edge → the empty cell IS to the right.
    # And the user says this bracket is drawn in the empty cell.
    # So "empty cell to the right of filled" gets right-edge brackets.

    # Let me re-read:
    # "If EMPTY and has a filled neighbor to the LEFT" → "L-bracket pair opening right"
    # That corresponds to "L-bracket at LEFT edge of filled region (empty cell to the left)"
    # Wait no... "filled neighbor to the LEFT" means the filled cell is to the LEFT,
    # so this is the "right edge of the filled region" → those are the "RIGHT edge" brackets.

    # OK let me just directly use the formulas from the user's description:

    # "L-bracket at RIGHT edge of filled region (empty cell to the right)"
    # = our case where the filled cell is to the LEFT of the current empty cell
    # = the current empty cell is to the RIGHT of the filled cell
    #
    # Bracket pair 1 (upper L):
    # M{x+w-bw} {y+h} V{y+5.56} H{x} V{y+4.70} H{x+w} V{y+h} H{x+w-bw} Z
    #
    # Bracket pair 2 (lower L):
    # M{x+w-2*bw} {y+7.44} H{x} V{y+6.58} H{x+w-bw} V{y+h} H{x+w-2*bw} V{y+7.44} Z

    paths = []

    # Bracket pair 1 (upper L) — right-edge bracket
    p1 = (
        f"M{fmt(x + w - BW)} {fmt(y + h)}"
        f"V{fmt(y + BRACKET_Y1)}"
        f"H{fmt(x)}"
        f"V{fmt(y + BRACKET_Y1 - BW)}"
        f"H{fmt(x + w)}"
        f"V{fmt(y + h)}"
        f"H{fmt(x + w - BW)}Z"
    )
    paths.append(p1)

    # Bracket pair 2 (lower L) — right-edge bracket
    p2 = (
        f"M{fmt(x + w - 2 * BW)} {fmt(y + BRACKET_Y2)}"
        f"H{fmt(x)}"
        f"V{fmt(y + BRACKET_Y2 - BW)}"
        f"H{fmt(x + w - BW)}"
        f"V{fmt(y + h)}"
        f"H{fmt(x + w - 2 * BW)}"
        f"V{fmt(y + BRACKET_Y2)}Z"
    )
    paths.append(p2)

    return paths


def right_bracket_pair(col, row):
    """
    L-bracket pair for an empty cell that has a filled neighbor to its RIGHT.
    Mirror of left_bracket_pair — brackets open to the left.

    These are the "L-bracket at LEFT edge of filled region (empty cell to the left)".
    """
    x, y = cell_xy(col, row)
    w = CELL_W
    h = CELL_H
    paths = []

    # Mirror of the right-edge brackets:
    # Bracket pair 1 (upper L):
    # M{x+bw} {y+5.56} V{y+h} H{x+bw*2} V{y+7.44} ... wait, user gave partial.
    # Let me mirror the right-edge formulas by flipping left↔right.
    #
    # Right-edge bracket 1:
    #   M{x+w-bw} {y+h} V{y+5.56} H{x} V{y+4.70} H{x+w} V{y+h} H{x+w-bw} Z
    #   Shape: vertical bar on right side (x+w-bw to x+w), horizontal bar at y+5.56 (x to x+w)
    #
    # Left-edge bracket 1 (mirror):
    #   Vertical bar on left side (x to x+bw), horizontal bar at y+5.56 (x to x+w)
    #   M{x+bw} {y+h} V{y+5.56} H{x+w} V{y+4.70} H{x} V{y+h} H{x+bw} Z

    p1 = (
        f"M{fmt(x + BW)} {fmt(y + h)}"
        f"V{fmt(y + BRACKET_Y1)}"
        f"H{fmt(x + w)}"
        f"V{fmt(y + BRACKET_Y1 - BW)}"
        f"H{fmt(x)}"
        f"V{fmt(y + h)}"
        f"H{fmt(x + BW)}Z"
    )
    paths.append(p1)

    # Right-edge bracket 2:
    #   M{x+w-2*bw} {y+7.44} H{x} V{y+6.58} H{x+w-bw} V{y+h} H{x+w-2*bw} V{y+7.44} Z
    #
    # Left-edge bracket 2 (mirror):
    #   M{x+2*bw} {y+7.44} H{x+w} V{y+6.58} H{x+bw} V{y+h} H{x+2*bw} V{y+7.44} Z

    p2 = (
        f"M{fmt(x + 2 * BW)} {fmt(y + BRACKET_Y2)}"
        f"H{fmt(x + w)}"
        f"V{fmt(y + BRACKET_Y2 - BW)}"
        f"H{fmt(x + BW)}"
        f"V{fmt(y + h)}"
        f"H{fmt(x + 2 * BW)}"
        f"V{fmt(y + BRACKET_Y2)}Z"
    )
    paths.append(p2)

    return paths


def vertical_bracket_bars(col, row):
    """
    Thin horizontal bars for an empty cell that has a filled neighbor above or below.
    These span the cell width at specific y offsets, creating the data-matrix look.
    """
    x, y = cell_xy(col, row)
    w = CELL_W
    paths = []

    # Two thin horizontal bars at the bracket y positions
    # Bar 1 at BRACKET_Y1
    by1 = y + BRACKET_Y1
    p1 = (
        f"M{fmt(x)} {fmt(by1 + BW)}"
        f"V{fmt(by1)}"
        f"H{fmt(x + w)}"
        f"V{fmt(by1 + BW)}"
        f"H{fmt(x)}Z"
    )
    paths.append(p1)

    # Bar 2 at BRACKET_Y2
    by2 = y + BRACKET_Y2
    p2 = (
        f"M{fmt(x)} {fmt(by2 + BW)}"
        f"V{fmt(by2)}"
        f"H{fmt(x + w)}"
        f"V{fmt(by2 + BW)}"
        f"H{fmt(x)}Z"
    )
    paths.append(p2)

    return paths


def generate_svg(text, fill_color):
    """Generate SVG content for the given text with the specified fill color."""
    grid, total_cols, num_rows = build_grid(text)
    all_paths = []

    for row in range(num_rows):
        for col in range(total_cols):
            if grid[row][col]:
                # Filled cell → solid rectangle
                all_paths.append(filled_rect_path(col, row))
            else:
                # Empty cell — check neighbors for bracket decorations
                has_left = col > 0 and grid[row][col - 1]
                has_right = col < total_cols - 1 and grid[row][col + 1]
                has_above = row > 0 and grid[row - 1][col]
                has_below = row < num_rows - 1 and grid[row + 1][col]

                if has_left:
                    # Filled neighbor to the left → right-edge brackets (opening left into empty)
                    all_paths.extend(left_bracket_pair(col, row))
                elif has_right:
                    # Filled neighbor to the right → left-edge brackets (opening right into empty)
                    all_paths.extend(right_bracket_pair(col, row))
                elif has_above or has_below:
                    # Vertical neighbor → thin horizontal bars
                    all_paths.extend(vertical_bracket_bars(col, row))

    # Calculate viewBox
    svg_w = total_cols * CELL_W
    svg_h = num_rows * (CELL_H - ROW_OVERLAP) + ROW_OVERLAP  # last row gets full height

    # Combine all path segments into a single path d attribute
    d = " ".join(all_paths)

    svg = (
        f'<svg viewBox="0 0 {fmt(svg_w)} {fmt(svg_h)}" '
        f'fill="none" xmlns="http://www.w3.org/2000/svg">\n'
        f'<path d="{d}" fill="{fill_color}"/>\n'
        f'</svg>'
    )
    return svg


def main():
    text = "Wickeban"

    output_dir = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "frontend", "public"
    )

    # Light mode (black on transparent)
    light_svg = generate_svg(text, "black")
    light_path = os.path.join(output_dir, "wickeban-logo.svg")
    with open(light_path, 'w') as f:
        f.write(light_svg)
    print(f"Written: {light_path} ({len(light_svg)} bytes)")

    # Dark mode (white on transparent)
    dark_svg = generate_svg(text, "white")
    dark_path = os.path.join(output_dir, "wickeban-logo-dark.svg")
    with open(dark_path, 'w') as f:
        f.write(dark_svg)
    print(f"Written: {dark_path} ({len(dark_svg)} bytes)")


if __name__ == "__main__":
    main()
