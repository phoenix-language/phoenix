import { red } from "../../../../deno/deps.ts";

const date = new Date();

/** Logs invalid type declarations */
export class IllegalTypeException extends Error {
    /**
    * @param error_message The error message to be displayed
    */
    constructor(error_message: string) {
        super(error_message);
        this.name = "IllegalTypeException";
        this.message =
            `[${date.toLocaleDateString()} ${date.toLocaleTimeString()}] ` +
            error_message;
    }
}

export class IllegalNullableTypeException extends Error {
    /**
     * @param error_message The error message to be displayed
     */
    constructor(error_message: string) {
        super(error_message);

        const err_name = "IllegalNullableTypeException";

        this.name = err_name;

        error_message =
            `[${date.toLocaleDateString()} ${date.toLocaleTimeString()}] ` +
            `${red(err_name)}: ${error_message}`;

        this.message = error_message;
    }
}

export class IllegalStateTypeException extends Error {
    /**
    * @param error_message The error message to be displayed
    */
    constructor(error_message: string) {
        super(error_message);
        this.name = "IllegalStateTypeException";
        this.message =
            `[${date.toLocaleDateString()} ${date.toLocaleTimeString()}] ` +
            error_message;
    }
}
