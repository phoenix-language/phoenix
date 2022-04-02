import { Position } from "./Position.ts";

/**
 * @param text The text to search.
 * @param start_position The position to start searching from.
 * @param end_position The position to stop searching at.
 */
export function parse_string_with_arrows(
    text: string,
    start_position: Position,
    end_position: Position,
): string {
    let result = "";

    let index_start = Math.max(text.lastIndexOf("\n", start_position.index), 0);
    let index_end = text.indexOf("\n", start_position.index) + 1;

    if (index_end < 0) index_end = text.length;

    const line_count = end_position.line - start_position.line + 1;

    for (let i = 0; i < line_count; i++) {
        const line = text.substring(index_start, index_end);
        let column_start;

        if (i === 0) {
            column_start = start_position.colum;
        } else {
            column_start = 0;
        }

        let colum_end;

        if (i === line_count - 1) {
            colum_end = end_position.colum;
        } else {
            colum_end = line.length;
        }

        result += line + "\n";
        result += " ".repeat(column_start);
        result += "^".repeat(colum_end - column_start);

        index_start = index_end;
        index_end = text.indexOf("\n", index_start) + 1;

        if (index_end < 0) index_end = text.length;
    }

    return result;
}
