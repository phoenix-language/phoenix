import { Exception } from "../exceptions/mod.ts";
import { Parser_Node } from "./nodes/Node.ts";

export class Parser_Result {
    public error: Exception | null;
    private last_registered_advance_count: number;
    public node: Parser_Node
    private advance_count: number;
    private to_reverse_count: number;

    public constructor() {
        this.error = null;
        // @ts-ignore - we can hack this
        this.node = null;
        this.last_registered_advance_count = 0;
        this.advance_count = 0;
        this.to_reverse_count = 0;
    }

    public generateAdvancement(): void {
        this.last_registered_advance_count = 1;
        this.advance_count++;
    }

    /**
     * @param result
     * @private
     */
    public generate(result: this): Parser_Node {
        this.last_registered_advance_count =
            result.last_registered_advance_count;
        this.advance_count += result.advance_count;
        if (result.error) {
            this.error = result.error;
        }
        return result.node;
    }

    /**
     * @param error
     * @private
     * @returns {Parser_Result}
     */
    public failure(error: Exception): this {
        if (!this.error || this.advance_count == 0) {
            this.error = error;
        }
        return this;
    }

    /**
     * @param node
     * @private
     * @returns {Parser_Result}
     */
    public success(node: Parser_Node): this {
        this.node = node;
        return this;
    }

    /**
     * @param {Parser_Result} result
     * @private
     * @returns {Parser_Node}
     */
    public tryGeneration(result: this) {
        if (result.error) {
            this.to_reverse_count = result.advance_count;
            return null;
        }

        return this.generate(result);
    }
}
