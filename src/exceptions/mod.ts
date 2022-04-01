import { Position } from "../lexer/Position.ts";
import { parse_string_with_arrows } from "../lexer/utilities.ts";

export class Core_Exception {
    public start_position: Position;
    public end_position: Position;
    public exception_name: string;
    public message: string;

    public constructor(
        start_position: Position,
        end_position: Position,
        exception_name: string,
        message: string,
    ) {
        this.start_position = start_position;
        this.end_position = end_position;
        this.exception_name = exception_name;
        this.message = message;
    }

    /**
     * Generates a string representation of the exception
     */
    public createException() {
        let result = `${this.exception_name}: ${this.message}`;
        result += `File: ${this.start_position.file_name}, at line ${
            this.start_position.line + 1
        }`;
        result += `\n\n ${
            parse_string_with_arrows(
                this.start_position.file_context,
                this.start_position,
                this.end_position,
            )
        }`;
        console.error(result);
    }
}

export class Illegal_Char_Exception extends Core_Exception {
    public constructor(
        start_position: Position,
        end_position: Position,
        message: string,
    ) {
        super(start_position, end_position, "Illegal Char Exception", message);
    }
}
