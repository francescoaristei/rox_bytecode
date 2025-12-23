use std::collections::HashMap;
use std::rc::Rc;
use crate::chunk::{Chunk, OP_ADD, OP_CONSTANT, OP_DIVIDE, OP_FALSE, OP_MULTIPLY, OP_NEGATE, OP_NIL, OP_RETURN, OP_SUBTRACT, OP_TRUE, OP_NOT, OP_GREATER, OP_EQUAL, OP_LESS};
use crate::scanner::{Scanner, Token, TokenType};
use crate::value::Value;
use crate::util::disassemble_chunk;

// values put in increasing order of precedence
#[derive(Copy, Clone, PartialOrd, PartialEq)]
enum Precedence {
    PrecedenceNone,
    PrecedenceAssignment,
    PrecedenceOr,
    PrecedenceAnd,
    PrecedenceEquality,
    PrecedenceComparison,
    PrecedenceTerm,
    PrecedenceFactor,
    PrecedenceUnary,
    PrecedenceCall,
    PrecedencePrimary
}

impl Precedence {
    pub fn next(precedence: Precedence) -> Precedence {
        match precedence {
            Precedence::PrecedenceNone => Precedence::PrecedenceAssignment,
            Precedence::PrecedenceAssignment => Precedence::PrecedenceOr,
            Precedence::PrecedenceOr => Precedence::PrecedenceAnd,
            Precedence::PrecedenceAnd => Precedence::PrecedenceEquality,
            Precedence::PrecedenceEquality => Precedence::PrecedenceComparison,
            Precedence::PrecedenceComparison => Precedence::PrecedenceTerm,
            Precedence::PrecedenceTerm => Precedence::PrecedenceFactor,
            Precedence::PrecedenceFactor => Precedence::PrecedenceUnary,
            Precedence::PrecedenceUnary => Precedence::PrecedenceCall,
            Precedence::PrecedenceCall => Precedence::PrecedencePrimary,
            Precedence::PrecedencePrimary => Precedence::PrecedenceNone
        }
    }
}

// this represents the function used to compile the expression represented by a token given by the Scanner:
// for example for TokenType::Minus obtained from -a.b the function self.unary() is associated.
type ParseFn<'a> = fn(&mut Parser<'a>);

// this is the building block which creates the association (TokenType, function) that allows to parse/compile the expression
// that a certain TokenType represents.
struct ParseRule<'a> {
    prefix: Option<ParseFn<'a>>,
    infix: Option<ParseFn<'a>>,
    // Here is defined Precedence value for the infix operator, used to retrieve the 1-level higher Precedence level,
    // so that in the example: 2 * 3 + 4 the self.binary() as right operand picks only '3' and not '3 + 4'.
    precedence: Precedence
}

impl<'a> ParseRule<'a> {
    pub fn new(prefix: Option<ParseFn<'a>>, infix: Option<ParseFn<'a>>, precedence: Precedence) -> ParseRule<'a> {
        ParseRule {
            prefix,
            infix,
            precedence
        }
    }

    pub fn get_precedence(&self) -> Precedence {
        self.precedence
    }
}

/*
 Compiler used to obtain (from the user source code) the bytecode that the Virtual Machine will then interpret.
 Is a single pass compiler, meaning it parses the code (consume tokens etc) and produces the bytecode, rather than
 parsing the code to produce the AST and then use the AST for bytecode generation.
 The parsing algorithm adopted is the Vaughan Pratt’s “top-down operator precedence parsing”.
 */
pub struct Parser<'a> {
    scanner: Scanner<'a>,
    current: Token<'a>,
    previous: Token<'a>,
    compiling_chunk: &'a mut Chunk,
    rules: HashMap<TokenType, ParseRule<'a>>,
    had_error: bool,
    panic_mode: bool
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str, chunk: &'a mut Chunk) -> Self {
        let mut rules_map = HashMap::new();
        rules_map.insert(TokenType::LeftParen, ParseRule::new(Some(Parser::grouping), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::RightParen, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::LeftBrace, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::RightBrace, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Comma, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Dot, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Minus, ParseRule::new(Some(Parser::unary), Some(Parser::binary), Precedence::PrecedenceTerm));
        rules_map.insert(TokenType::Plus, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceTerm));
        rules_map.insert(TokenType::Semicolon, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Slash, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceFactor));
        rules_map.insert(TokenType::Star, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceFactor));
        rules_map.insert(TokenType::Bang, ParseRule::new(Some(Parser::unary), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::BangEqual, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceEquality));
        rules_map.insert(TokenType::Equal, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::EqualEqual, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceEquality));
        rules_map.insert(TokenType::Greater, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceComparison));
        rules_map.insert(TokenType::GreaterEqual, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceComparison));
        rules_map.insert(TokenType::Less, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceComparison));
        rules_map.insert(TokenType::LessEqual, ParseRule::new(None, Some(Parser::binary), Precedence::PrecedenceComparison));
        rules_map.insert(TokenType::Identifier, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::String, ParseRule::new(Some(Parser::string), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Number, ParseRule::new(Some(Parser::number), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::And, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Class, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Else, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::False, ParseRule::new(Some(Parser::literal), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::For, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Fun, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::If, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Nil, ParseRule::new(Some(Parser::literal), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Or, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Print, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Return, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Super, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::This, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::True, ParseRule::new(Some(Parser::literal), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Var, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::While, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Error, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Eof, ParseRule::new(None, None, Precedence::PrecedenceNone));

        Parser {
            scanner: Scanner::new(source),
            current: Token::new("current"),
            previous: Token::new("previous"),
            compiling_chunk: chunk,
            rules: rules_map,
            had_error: false,
            panic_mode: false
        }
    }

    pub fn compile(&mut self) -> bool {
        self.advance();
        self.expression();
        self.consume(TokenType::Eof, "Expect end of expression.");
        self.end_compiler();
        !self.had_error
    }

    /*
    Here the code connecting parsing utilities (advance(), consume(), error_at()..) with code generation
    utilities (emit_byte(), emit_bytes()...).
     */
    fn expression(&mut self) {
        // compile the lowest precedence expression, which in turn will compile also all the higher level ones contained within it.
        self.parse_precedence(Precedence::PrecedenceAssignment);
    }

    fn unary(&mut self) {
        let token_type = self.previous.get_token_type();

        /*
        Note that first the expression gets compiled and its bytecode added in the stack,
        and only after that the negate instruction bytecode is added in the stack. This makes sense,
        first the operand is evaluated and then negated. The compiler parse the code as written by the user
        but then re-arranges it in the correct execution order.
         */
        self.parse_precedence(Precedence::PrecedenceUnary);

        match token_type {
            TokenType::Minus => self.emit_byte(OP_NEGATE),
            TokenType::Bang => self.emit_byte(OP_NOT),
            _ => unreachable!()
        }
    }

    fn string(&mut self) {
        let lexeme = self.previous.get_lexeme();
        self.emit_constant(Value::String(Rc::new(lexeme[1..lexeme.len() - 1].to_string())))
    }

    /*
    A grouping expression allows to insert a lower precedence expression where a higher precedence one is expected,
    but it does not have a runtime semantic meaning for the backend, therefore no bytecode is emitted for it, but only for
    the expression within it.
     */
    fn grouping(&mut self) {
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after expression");
    }

    fn number(&mut self) {
        let value: f64 = self.previous.
            get_lexeme()
            .parse()
            .expect("Parsed value is not a double.");
        self.emit_constant(Value::Number(value));
    }

    /*
    In case of an operation like a + b, self.number() first consumes the 'a' operand and emits a OP_CONSTANT bytecode instruction
    at this point, the '+' infix operator is consumed by the parser. Using the '+' the compiler can understand that the
    'a' operand pushed on the stack before is an operand to the addition operation is parsing.
    The VM will execute the left and right operands on the stack and then the instruction for the operator: it will pop the two values
    from the stack, execute the operation and push the result on the stack.
     */
    fn binary(&mut self) {
        // at the time binary() is called, the infix operator has been consumed and is in the self.previous token
        let operator_type = self.previous.get_token_type();
        let parse_rule = self.get_rule(&operator_type);
        let precedence = parse_rule.get_precedence();
        self.parse_precedence(Precedence::next(precedence));
        match operator_type {
            // a != b is equal to !(a == b)
            TokenType::BangEqual => self.emit_bytes(OP_EQUAL, OP_NOT),
            TokenType::EqualEqual => self.emit_byte(OP_EQUAL),
            TokenType::Greater => self.emit_byte(OP_GREATER),
            TokenType::GreaterEqual => self.emit_bytes(OP_LESS, OP_NOT),
            TokenType::Less => self.emit_byte(OP_LESS),
            TokenType::LessEqual => self.emit_bytes(OP_GREATER, OP_NOT),
            TokenType::Plus => self.emit_byte(OP_ADD),
            TokenType::Minus => self.emit_byte(OP_SUBTRACT),
            TokenType::Star => self.emit_byte(OP_MULTIPLY),
            TokenType::Slash => self.emit_byte(OP_DIVIDE),
            _ => unreachable!()
        }
    }

    fn literal(&mut self) {
        match self.previous.get_token_type() {
            TokenType::False => self.emit_byte(OP_FALSE),
            TokenType::True => self.emit_byte(OP_TRUE),
            TokenType::Nil => self.emit_byte(OP_NIL),
            _ => unreachable!()
        }
    }

    fn get_rule(&mut self, operator_type: &TokenType) -> &ParseRule<'a> {
        self.rules.get(operator_type).unwrap()
    }

    /*
    That is the core of the Vaughan Pratt’s “top-down operator precedence parsing” algorithm.
    Example:
    Why precedence <= current_precedence? means "parse an expression, but STOP when you hit an operator with precedence LOWER than prec"
    This automatically handles:
    1. Operator precedence (* binds tighter than +)
    2. Left-associativity (1 + 2 + 3 groups as (1 + 2) + 3)

    Example: Parsing 1 + 2 * 3

     Step 1: Start

      Tokens: [1] [+] [2] [*] [3]
               ^
            previous = (none)
            current = NUMBER(1)

      Call: parse_precedence(PREC_ASSIGNMENT)  // precedence = 1

      Step 2: Consume prefix (number)

      advance()
      Tokens: [1] [+] [2] [*] [3]
                   ^
            previous = NUMBER(1)
            current = PLUS

      Execute prefix rule for NUMBER(1):
        - Emit: OP_CONSTANT(1)

      Step 3: Check infix loop

      while (PREC_ASSIGNMENT <= precedence_of(PLUS))
        //   1                <= 6 (PREC_TERM)
        //   TRUE! Continue...

      Step 4: Consume infix (+)

      advance()
      Tokens: [1] [+] [2] [*] [3]
                       ^
            previous = PLUS
            current = NUMBER(2)

      Execute infix rule for PLUS:
        - Call binary() function

      ---
      Inside binary() for +
       fn binary(&mut self) {
          let operator_type = self.previous.get_token_type();
          let rule = self.get_rule(operator_type);

          // Parse the right operand with HIGHER precedence
          // For left-associative: use precedence + 1
          self.parse_precedence(rule.precedence + 1);  // PREC_TERM + 1 = PREC_FACTOR (7)

          // Emit the operator
          self.emit_byte(OP_ADD);
      }

      This recursive call is crucial! We call parse_precedence(PREC_FACTOR) to parse the right side.

      ---
      Step 5: Recursive call - parse right side of +

      Call: parse_precedence(PREC_FACTOR)  // precedence = 7

      advance()
      Tokens: [1] [+] [2] [*] [3]
                           ^
            previous = NUMBER(2)
            current = STAR

      Execute prefix rule for NUMBER(2):
        - Emit: OP_CONSTANT(2)

      Step 6: Check infix loop (inside recursive call)

      while (PREC_FACTOR <= precedence_of(STAR))
        //   7            <= 7 (PREC_FACTOR)
        //   TRUE! Continue...

      Step 7: Consume infix (*)

      advance()
      Tokens: [1] [+] [2] [*] [3]
                               ^
            previous = STAR
            current = NUMBER(3)

      Execute infix rule for STAR:
        - Call binary() function

      ---
      Inside binary() for *

      fn binary(&mut self) {
          let operator_type = self.previous.get_token_type();
          let rule = self.get_rule(operator_type);

          self.parse_precedence(rule.precedence + 1);  // PREC_FACTOR + 1 = PREC_UNARY (8)

          self.emit_byte(OP_MULTIPLY);
      }

      ---
      Step 8: Another recursive call

      Call: parse_precedence(PREC_UNARY)  // precedence = 8

      advance()
      Tokens: [1] [+] [2] [*] [3] [EOF]
                                   ^
            previous = NUMBER(3)
            current = EOF

      Execute prefix rule for NUMBER(3):
        - Emit: OP_CONSTANT(3)

      Step 9: Check infix loop

      while (PREC_UNARY <= precedence_of(EOF))
        //   8           <= 0 (PREC_NONE)
        //   FALSE! Exit loop and return

      Returns from parse_precedence(PREC_UNARY)

      ---
      Step 10: Back in binary() for *

      // Right side is parsed, now emit the operator
      self.emit_byte(OP_MULTIPLY);  // Emit: OP_MULTIPLY

      Returns from binary() for *

      ---
      Step 11: Back in the while loop (Step 6)

      Tokens: [1] [+] [2] [*] [3] [EOF]
                                   ^
            current = EOF

      Check: while (PREC_FACTOR <= precedence_of(EOF))
             //     7            <= 0
             //     FALSE! Exit loop and return

      Returns from parse_precedence(PREC_FACTOR) - the recursive call from +

      ---
      Step 12: Back in binary() for +

      // Right side is parsed, now emit the operator
      self.emit_byte(OP_ADD);  // Emit: OP_ADD

      Returns from binary() for +

      ---
      Step 13: Back in original while loop (Step 3)

      Check: while (PREC_ASSIGNMENT <= precedence_of(EOF))
             //     1                <= 0
             //     FALSE! Exit and return

      Done!

      ---
      Bytecode Emitted (in order)

      OP_CONSTANT(1)    // Push 1
      OP_CONSTANT(2)    // Push 2
      OP_CONSTANT(3)    // Push 3
      OP_MULTIPLY       // 2 * 3 = 6
      OP_ADD            // 1 + 6 = 7

      This correctly evaluates as 1 + (2 * 3) = 7 ✅

      ---
      The Magic of precedence + 1

      Notice in binary() we call parse_precedence(rule.precedence + 1):

      self.parse_precedence(rule.precedence + 1);

      This is for left-associativity:
      - When parsing the right side of +, we use PREC_TERM + 1 = PREC_FACTOR
      - This means: "don't consume another + on the right"
      - So 1 + 2 + 3 parses as (1 + 2) + 3, not 1 + (2 + 3)

      For right-associative operators (like =), you'd use the same precedence:
      self.parse_precedence(rule.precedence);  // No + 1
     */
    fn parse_precedence(&mut self, precedence: Precedence) {
        /*
        The first token, by definition, is always going to belong to some prefix expression.
         */
        self.advance();
        let token_type = self.previous.get_token_type();
        let prefix_rule = self.get_rule(&token_type).prefix;

        if let Some(prefix) = prefix_rule {
            prefix(self);
        } else {
            self.error("Expect expression.");
            return;
        }

        /*
        If we find an infix expression, it means that the prefix expression computed in the first part
        is an operand for it.
        After an iteration we check again if there is an infix operator, in which case the whole previous infix
        operator could be the left operand to it.
        The algorithm goes until there isn't an infix operator or an operator with too low precedence is encountered.
         */
        while precedence <= self.get_rule(&self.current.get_token_type()).precedence {
            self.advance();
            let infix_rule = self.get_rule(&self.previous.get_token_type()).infix.unwrap();
            // the infix operator consume the right token and compile to the bytecode for its operation.
            infix_rule(self);
        }
    }

    // add the value in the constants table and emits a OP_CONSTANT instruction
    // to push the code in the stack at runtime.
    fn emit_constant(&mut self, value: Value) {
        let index = self.make_constant(value);
        self.emit_bytes(OP_CONSTANT, index)
    }

    // compiles a number literal to bytecode
    fn make_constant(&mut self, value: Value) -> u8 {
        let index = self.current_chunk().add_constant(value);
        match u8::try_from(index) {
            Ok(_) => index as u8,
            Err(_) => {
                self.error("Too many constant in one chunk.");
                0
            }
        }
    }

    fn end_compiler(&mut self) {
        #[cfg(feature = "debug-trace-execution")]
        {
            if !self.had_error {
                disassemble_chunk(self.current_chunk(), "code");
            }
        }
        self.emit_return();
    }

    fn emit_return(&mut self) {
        self.emit_byte(OP_RETURN);
    }

    // for writing OP_CODE + operand
    fn emit_bytes(&mut self, byte1: u8, byte2: u8) {
        self.emit_byte(byte1);
        self.emit_byte(byte2);
    }

    fn emit_byte(&mut self, byte: u8) {
        let line = self.previous.get_line();
        self.current_chunk().write(byte, line);
    }

    fn current_chunk(&mut self) -> &mut Chunk {
        self.compiling_chunk
    }

    fn advance(&mut self) {
        self.previous = self.current;

        loop {
            self.current = self.scanner.scan_token();
            if self.current.get_token_type() != TokenType::Error {
                break;
            }
            self.error_at_current(self.current.get_lexeme());
        }
    }

    fn consume(&mut self, token_type: TokenType, message: &str) {
        if self.current.get_token_type() == token_type {
            self.advance();
            return
        }
        self.error_at_current(message)
    }

    fn error_at_current(&mut self, message: &str) {
        self.error_at(self.current, message);
    }

    fn error(&mut self, message: &str) {
        self.error_at(self.previous, message);
    }

    fn error_at(&mut self, token: Token, message: &str) {
        // if already in panic mode no more error should be presented to the user
        if self.panic_mode {
            return
        }
        self.panic_mode = true;
        eprint!("[line {}] Error", token.get_line());
        match token.get_token_type() {
            TokenType::Eof => eprint!("at end."),
            TokenType::Error => (),
            _ => eprint!("at {}", token.get_lexeme())
        }
        eprint!(":{}", message);
        self.had_error = true;
    }
}