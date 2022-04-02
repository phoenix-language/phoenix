import Token from "../../lexer/Token.ts";
import { Parser_Node } from "./Node.ts";

export class StringNode extends Parser_Node {
    private token: Token;

    public constructor(_token: Token) {
        super();
        this.token = _token;

        this.start_position = _token.position_start;
        this.end_position = _token.position_end;

        this.name = "StringNode";
    }

    public toString(): string | string[] {
        return this.token.toString();
    }
}
