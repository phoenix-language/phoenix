import Token from "../../lexer/Token.ts";
import { Parser_Node } from "./Node.ts";

export class IfCaseNode extends Parser_Node {
    private case: Array<Array<Parser_Node>>;
    private elseCase: Parser_Node;

    public constructor(
        _cases: Array<Array<Parser_Node>>,
        elseCases: Parser_Node,
    ) {
        super();

        this.case = _cases;
        this.elseCase = elseCases;

        this.start_position = this.case[0][0].start_position;
        this.end_position =
            (this.elseCase || this.case[this.case.length - 1][0]).end_position;

        this.name = "IfCaseNode";
    }
}
