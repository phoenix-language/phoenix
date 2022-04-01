import { Position } from "./Position.ts";
import {
    Internal_Constants,
    Internal_TokeTypes,
} from "./constants/const_token.ts";
import Token from "./Token.ts";
import { Illegal_Char_Exception } from "../exceptions/mod.ts";

/**
 * Internal Lexer class which tokenized our code.
 */
export class Lexer {
    public file_name: string;
    public file_contents: string;
    public current_char: string | null;
    public current_Position: Position;

    /**
     * @param _file_name The name of the file we are tokenizing.
     * @param _file_contents The contents of the file we are tokenizing.
     */
    public constructor(_file_name: string, _file_contents: string) {
        this.file_name = _file_name;
        this.file_contents = _file_contents;
        this.current_char = null;

        this.current_Position = new Position(
            -1,
            0,
            -1,
            _file_name,
            _file_contents,
        );
        this.advance();
    }

    /**
     * Advance the current_char and current_Position
     * @private
     */
    private advance(): void;
    private advance() {
        this.current_Position.advance(this.current_char as string);
        // If index position is less than the data length, we have not finished reading the file.
        this.current_char =
            this.current_Position.index < this.file_contents.length
                ? this.file_contents[this.current_Position.index]
                : null;
    }

    /**
     * Handles the lexer tokens and parsing of the str to a token.
     */
    public generateTokens() {
        const tokens = [];

        while (
            this.current_char != null
        ) {
            if (" \t\r".includes(this.current_char)) {
                this.advance();
            } else if ("\n;".includes(this.current_char)) {
                tokens.push(
                    new Token(
                        Internal_TokeTypes.NEWLINE,
                        null,
                        this.current_Position,
                    ),
                );
                this.advance();
            } else if (Internal_Constants.numbers.includes(this.current_char)) {
                tokens.push(this.generateNumber());
            } else if (Internal_Constants.letters.includes(this.current_char)) {
                tokens.push(this.generateIdentifier());
            } else if (this.current_char === "+") {
                tokens.push(
                    new Token(
                        Internal_TokeTypes.PLUS,
                        "+",
                        this.current_Position,
                    ),
                );
                this.advance()
            } else if (this.current_char === "-") {
                tokens.push(
                    new Token(
                        Internal_TokeTypes.MINUS,
                        null,
                        this.current_Position,
                    ),
                );
                this.advance()
            } else if (this.current_char === "*") {
                tokens.push(
                    new Token(
                        Internal_TokeTypes.MULTIPLY,
                        null,
                        this.current_Position,
                    ),
                );
                this.advance()
            } else if (this.current_char === "/") {
                tokens.push(
                    new Token(
                        Internal_TokeTypes.DIVIDE,
                        null,
                        this.current_Position,
                    ),
                );
                this.advance()
            }
            else if (this.current_char === "(") {
                tokens.push(
                    new Token(
                        Internal_TokeTypes.LPAREN,
                        null,
                        this.current_Position,
                    ),
                );
                this.advance()
            }
            else if (this.current_char === ")") {
                tokens.push(
                    new Token(
                        Internal_TokeTypes.RPAREN,
                        null,
                        this.current_Position,
                    ),
                );
                this.advance()
            }
            else {
                const start_position = this.current_Position.copy();
                const char = this.current_char;
                this.advance();
                return {
                    tokens: null,
                    error: new Illegal_Char_Exception(
                        start_position,
                        this.current_Position,
                        `Invalid character: ${char} at ${start_position.toString()}`,
                    ),
                };
            }
        }
        tokens.push(
            new Token(Internal_TokeTypes.EOF, null, this.current_Position),
        );
        return {
            tokens: tokens,
            error: null,
        };
    }

    private generateIdentifier() {
        let str_id = ""
        const start_position = this.current_Position.copy();
        const letter_numbers = Internal_Constants.letters + Internal_Constants.numbers;
        while (
            this.current_char != null &&
            letter_numbers.includes(this.current_char)
        ) {
            str_id += this.current_char;
            this.advance();
        }

        const toke_type = Internal_TokeTypes.KEYWORDS.includes(str_id)
            ? Internal_TokeTypes.KEYWORDS
            : Internal_TokeTypes.IDENTIFIER;

        return new Token(
            toke_type,
            str_id,
            start_position,
            this.current_Position
        );
    }

    private generateNumber(): Token {
        let number_str = "";
        let dot_count = 0;
        const start_position = this.current_Position.copy();
        let digits = Internal_Constants.numbers;
        digits += ".";

        while (this.current_char && digits.includes(this.current_char)) {
            if (this.current_char == ".") {
                dot_count++;
            }
            number_str += this.current_char;
            this.advance();
        }

        if (dot_count === 0) {
            // Integer
            return new Token(
                Internal_TokeTypes.INT,
                number_str,
                start_position,
                this.current_Position,
            );
        } else {
            // We have a float
            return new Token(
                Internal_TokeTypes.FLOAT,
                number_str,
                start_position,
                this.current_Position,
            );
        }
    }
}
