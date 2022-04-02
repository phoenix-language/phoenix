import { Position } from "../../lexer/Position.ts";

/**
 * A node in the syntax tree.
 */
export class Parser_Node {
    public start_position: Position | null;
    public end_position: Position | null;
    /** The name of the node. */
    public name: string;

    public constructor() {
        this.start_position = null;
        this.end_position = null;
        this.name = "";
    }
}
