/**
 * Internal Position Utility for the Lexer.
 * Helps track the current position in the text to be lexed.
 */
export class Position {
    /**
     * @param index The position in the input stream.
     * @param line The line in the input stream.
     * @param colum The column in the input stream.
     * @param file_name The name of the file.
     * @param file_context The file content to lex.
     */
    public constructor(
        index: number,
        line: number,
        colum: number,
        file_name: string,
        file_context: string,
    ) {
        this.index = index;
        this.line = line;
        this.colum = colum;
        this.file_name = file_name;
        this.file_context = file_context;
    }

    /**
     * Index of the current position in the input stream.
     * @param char The character to add to the input stream.
     */
    public advance(char: string): void;
    public advance(char: string) {
        if (char === "\n") {
            this.line++;
            this.colum = 0;
        } else {
            this.colum++;
        }
        this.index++;
    }

    /**
     * Returns the current position in the input stream.
     */
    public copy(): Position;
    public copy() {
        return new Position(
            this.index,
            this.line,
            this.colum,
            this.file_name,
            this.file_context,
        );
    }

    public toString(): string | string[] {
        if (this.file_name) {
            return `Colum ${this.colum}, Line ${this.line} in ${this.file_name}`;
        } else {
            return this.file_name;
        }
    }

    /** The column in the input stream. */
    public colum: number;
    /** The file content to lex. */
    public file_context: string;
    /** The name of the file. */
    public file_name: string;
    /** index The position in the input stream. */
    public index: number;
    /** The line in the input stream. */
    public line: number;
}
