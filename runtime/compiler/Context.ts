import {Position} from "../lexer/Position.ts";

export class Context {
    public display_name: string
    public parent: Context
    public parent_entity_position: Position
    public constructor(display_name: string, parent: Context, parent_entity_position: Position) {
        this.display_name = display_name
        this.parent = parent
        this.parent_entity_position = parent_entity_position
    }
}