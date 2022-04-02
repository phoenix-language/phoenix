import Token from "../../lexer/Token.ts";
import { Parser_Node } from "./Node.ts";

export class OperationNode extends Parser_Node {
    private left_node: Parser_Node;
    private op_token: Token;
    private right_node: Parser_Node;

    public constructor(
        left_node: Parser_Node | null,
        op_token: Token,
        right_node: Parser_Node | null,
    ) {
        super();

        this.left_node = left_node!;
        this.op_token = op_token;
        this.right_node = right_node!;

        this.start_position = this.left_node.start_position;
        this.end_position = this.right_node.end_position;

        this.name = "OperationNode";
    }

    public toString(): string | string[] {
        return `${this.left_node.toString()}, ${this.op_token.toString()}, ${this.right_node.toString()}`;
    }
}
