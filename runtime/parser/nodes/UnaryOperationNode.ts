import Token from "../../lexer/Token.ts";
import { Parser_Node } from "./Node.ts";

export class UnaryOperationNode extends Parser_Node {
    private node: Parser_Node;
    private op_token: Token;

    public constructor(node: Parser_Node, op_token: Token) {
        super();

        this.node = node;
        this.op_token = op_token;

        this.start_position = node.start_position;
        this.end_position = node.end_position;

        this.name = "UnaryOperationNode";
    }

    public toString(): string | string[] {
        return `(${this.op_token.toString()} ${this.node.toString()})`;
    }
}
