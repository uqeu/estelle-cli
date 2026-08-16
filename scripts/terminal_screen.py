"""Small ANSI screen emulator for public-binary pseudo-terminal probes."""

from __future__ import annotations


def _escape_end(text: str, index: int, terminator: str) -> int:
    end = index
    while end < len(text):
        if terminator == "csi" and "@" <= text[end] <= "~":
            return end
        if terminator == "osc" and text[end] == "\a":
            return end + 1
        if terminator == "osc" and text[end : end + 2] == "\x1b\\":
            return end + 2
        end += 1
    return len(text)


def _apply_csi(screen, row, column, raw, final, rows, columns):
    params = raw.lstrip("?<>!")
    values = [int(value) if value.isdigit() else 1 for value in params.split(";")]
    amount = values[0] if values else 1
    if final in ("H", "f"):
        row = max(0, min(rows - 1, amount - 1))
        column = max(0, min(columns - 1, (values[1] if len(values) > 1 else 1) - 1))
    elif final == "G":
        column = max(0, min(columns - 1, amount - 1))
    elif final == "A":
        row = max(0, row - amount)
    elif final == "B":
        row = min(rows - 1, row + amount)
    elif final == "C":
        column = min(columns - 1, column + amount)
    elif final == "D":
        column = max(0, column - amount)
    elif final == "J" and raw == "2":
        screen = [[" "] * columns for _ in range(rows)]
    elif final == "K":
        screen[row][column:] = [" "] * (columns - column)
    return screen, row, column


def rendered_screen(data: bytes, rows: int = 30, columns: int = 120) -> str:
    screen = [[" "] * columns for _ in range(rows)]
    row = 0
    column = 0
    text = data.decode("utf-8", errors="ignore")
    index = 0
    while index < len(text):
        character = text[index]
        if character == "\x1b":
            if index + 1 < len(text) and text[index + 1] == "[":
                end = _escape_end(text, index + 2, "csi")
                if end >= len(text):
                    break
                raw = text[index + 2 : end]
                screen, row, column = _apply_csi(
                    screen, row, column, raw, text[end], rows, columns
                )
                index = end + 1
                continue
            if index + 1 < len(text) and text[index + 1] == "]":
                index = _escape_end(text, index + 2, "osc")
                continue
            index += 2
            continue
        if character == "\r":
            column = 0
        elif character == "\n":
            row = min(rows - 1, row + 1)
        elif character == "\b":
            column = max(0, column - 1)
        elif character >= " ":
            screen[row][column] = character
            column = min(columns - 1, column + 1)
        index += 1
    return "\n".join("".join(line).rstrip() for line in screen)
