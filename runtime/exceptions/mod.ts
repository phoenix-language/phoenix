import { Position } from "../lexer/Position.ts";
import { parse_string_with_arrows } from "../lexer/Utilities.ts";
import {Context} from "../compiler/Context.ts";

export class Exception {
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
        let result = `[${this.exception_name}] ${this.message}`;
        // result += `File: ${this.start_position.file_name}, at line ${
        //     this.start_position.line + 1
        // }`;
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

/**
 * Handles invalid characters in your code and throws and error.
 */
export class Illegal_Char_Exception extends Exception {
    public constructor(
        start_position: Position,
        end_position: Position,
        message: string,
    ) {
        super(start_position, end_position, "Illegal Char Exception", message);
    }
}

/**
 * Handles invalid Syntax in your code and throws and error.
 */
export class Illegal_Syntax_Exception extends Exception {
    public constructor(
        start_position: Position,
        end_position: Position,
        message: string,
    ) {
        super(
            start_position,
            end_position,
            "Illegal Syntax Exception",
            message,
        );
    }
}

/**
 * Handles invalid runtime errors in your code and throws and error.
 */
export class Illegal_Runtime_Exception extends Exception {
    private context: Context;

    /**
     * @param start_position
     * @param end_position
     * @param message
     * @param context
     */
    public constructor(
        start_position: Position,
        end_position: Position,
        message: string,
        context: Context
    ) {
        super(
            start_position,
            end_position,
            "Illegal Runtime Exception",
            message,
        );
        this.context = context;
    }

    public createException() {
        let result = this.generateTrackBase()
        result += `${this.exception_name}: ${this.message}`;
        result += `\n\n ${
            parse_string_with_arrows(
                this.start_position.file_context,
                this.start_position,
                this.end_position,
            )
        }`;
        console.error(result);
    }

    private generateTrackBase() {
        let result = "";
        let pos = this.start_position;
        let ctx = this.context;

        while(ctx ) {
            result = `  File ${pos.line}, line ${pos.line + 1}, in ${ctx.display_name}\n ${result}`;
            pos = ctx.parent_entity_position;
            ctx = ctx.parent;
        }

        return `Traceback (most recent call last):\n$` + result;
    }
}
