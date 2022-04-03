export interface Tokenizer {
  /**
   * Starts the tokenizer
   * @param string_to_tokenize
   * @returns {boolean} boolean
   */
  init(string_to_tokenize: string): boolean;

  /** Checks if the token is at the end of the file */
  isEOF(): boolean;

  /** If there are more tokens to parse */
  has_more_tokens(): boolean;

  /** Finds the next token to tokenize */
  get_next_token(): Token | null;
}

export interface Token {
  /** The token type, stored in a javascript string value */
  type: string;

  /** The token value, stored in a javascript string. */
  value: string;
}
