import Token from "../../lexer/Token.ts";
import { Parser_Node } from "./Node.ts";

export class FunctionNode extends Parser_Node {
    private readonly token_name: Token;
    private token_args: Token[];
    private node: Parser_Node;
    private should_return_null: boolean | null;

    public constructor(
        token_name: Token,
        token_args: Token[],
        node: Parser_Node,
        should_return_null: boolean | null,
    ) {
        super();

        this.token_name = token_name;
        this.token_args = token_args;
        this.node = node;
        this.should_return_null = should_return_null;

        if (this.token_name) {
            this.start_position = this.token_name.position_start;
        } else if (this.token_args.length > 0) {
            this.start_position = this.token_args[0].position_start;
        } else {
            this.start_position = node.start_position;
        }

        this.end_position = this.node.end_position;

        this.name = "FunctionNode";
    }
}
