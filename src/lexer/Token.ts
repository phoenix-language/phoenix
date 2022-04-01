import { Position } from "./Position.ts";

export default class Token {
    public type: string;
    public value: string | null;
    public position_start: Position;
    public position_end: Position | undefined;

    /**
     * @param type The type of the token
     * @param value The value of the token
     * @param _position_start The starting position of the token
     * @param _position_end The ending position of the token
     */
    public constructor(
        type: string,
        value: string | null,
        _position_start: Position,
        _position_end?: Position,
    ) {
        this.type = type;
        this.value = value;

        // If the position is not undefined, it means we have a valid position in the que.
        if (_position_start) {
            this.position_start = _position_start.copy();
            this.position_end = _position_start.copy();
            //@ts-ignore - ignore for now
            this.position_end.advance();
        }

        if (_position_end) {
            this.position_end = _position_end;
        }

        this.position_start = _position_start;
        this.position_end = _position_end;
    }

    /**
     * Checks if the type of the token is equal to the given type.
     * @param expected_type
     * @param expected_value
     * @returns {boolean}
     */
    public matches(expected_type: string, expected_value: string): boolean {
        return this.type === expected_type && this.value === expected_value;
    }

    public toString(): string {
        if (this.value) {
            return `${this.type}:${this.value}`;
        } else {
            return this.type;
        }
    }
}
