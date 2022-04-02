import Token from "../lexer/Token.ts";
import { Internal_TokenTypes } from "../lexer/constants/const_token.ts";
import { Illegal_Syntax_Exception } from "../exceptions/mod.ts";
import { Parser_Result } from "./Result.ts";
import { NumberNode, OperationNode, UnaryOperationNode } from "./nodes/mod.ts";

export class Parser {
    private readonly tokens: Token[];
    private token_index: number;
    private current_token: Token;

    public constructor(_tokens: Token[]) {
        this.tokens = _tokens;
        this.token_index = 0;
        this.current_token = this.tokens[this.token_index];
        this.advance();
    }

    private advance(): Token {
        this.token_index++;
        this.updateTokenQueue();
        return this.tokens[this.token_index - 1];
    }

    private updateTokenQueue(): void {
        if (this.token_index >= 0 && this.token_index < this.tokens.length) {
            this.current_token = this.tokens[this.token_index];
        }
    }


    public parse(): Parser_Result {
        const result = this.expression();
        // @ts-ignore - this is a hack to get the parser to work
        if (result.error && this.current_token != Internal_TokenTypes.EOF) {
            return result.failure(
                new Illegal_Syntax_Exception(
                    this.current_token.position_start,
                    this.current_token.position_end,
                    "Expected '+', '-', '*', '/', '(' or ')' but got '" +
                        this.current_token.value + "'",
                ),
            );
        }

        return result;
    }

    private binOp(
        function_a: Function,
        operations: Array<string>,
        function_b?: Function,
    ): Parser_Result  {
        if (function_b === undefined) {
            function_b = function_a;
        }

        const result = new Parser_Result();
        let left = result.generate(function_a());
        if (result.error) {
            return result;
        }

        while (
            operations.includes(this.current_token.type) ||
            operations.includes(
                this.current_token.type + this.current_token.value,
            )
        ) {
            const operation_token = this.current_token;
            result.generateAdvancement();
            this.advance();

            const right = result.generate(function_b());

            if (result.error) {
                return result;
            }

            left = new OperationNode(left, operation_token, right);
        }

        return result.success(left);
    }

    private term() {
        return this.binOp(
            this.parse_factors.bind(this),
            [Internal_TokenTypes.MULTIPLY, Internal_TokenTypes.DIVIDE],
        );
    }

    private expression() {
        return this.binOp(
            this.term.bind(this),
            [Internal_TokenTypes.PLUS, Internal_TokenTypes.MINUS],
        );
    }

    // Grammar Rules

    private parse_factors() {
        const result = new Parser_Result();
        const token = this.current_token;
        if (
            [Internal_TokenTypes.PLUS, Internal_TokenTypes.MINUS].includes(
                this.current_token.type,
            )
        ) {
            result.generateAdvancement();
            this.advance();
            const factor = result.generate(this.parse_factors());
            if (result.error) return result;
            // @ts-ignore - this is a hack to get the parser to work
            return result.success(new UnaryOperationNode(token, factor));
        } else if (
            [Internal_TokenTypes.INT, Internal_TokenTypes.FLOAT].includes(
                this.current_token.type,
            )
        ) {
            result.generateAdvancement();
            this.advance();
            return result.success(new NumberNode(token));
        } else if (token.type == Internal_TokenTypes.LPAREN) {
            result.generateAdvancement();
            this.advance();
            const expr = result.generate(this.expression());
            if (result.error) return result;
            if (this.current_token.type == Internal_TokenTypes.RPAREN) {
                result.generateAdvancement();
                this.advance();
                return result.success(expr);
            } else {
                return result.failure(
                    new Illegal_Syntax_Exception(
                        this.current_token.position_start,
                        this.current_token.position_end,
                        "Expected ')' but got '" + this.current_token.value +
                            "'",
                    ),
                );
            }
        } else {
            // If no expression is found, return an error
            return result.failure(
                new Illegal_Syntax_Exception(
                    this.current_token.position_start,
                    this.current_token.position_end,
                    "Expected an int or float type number, '+', '-', '(', or ')' but got '" +
                        this.current_token.value + "'",
                ),
            );
        }
    }
}
