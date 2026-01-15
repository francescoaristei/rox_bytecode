use std::collections::HashMap;
use std::rc::Rc;
use crate::chunk::{Chunk, OpCode};
use crate::compiler::Precedence::PrecedenceAssignment;
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
type ParseFn<'a> = fn(&mut Parser<'a>, can_assign: bool);

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
Local variables are resolved at compile time using the stack.
The stack is used to store temporary variables needed to compute expressions.
Statements don't interfere with expressions, therefore is safe to store local variables there.
Statements (blocks and function bodies in this case) are nested and when a block ends the local variables defined within it go out of scope
as well, therefore the stack fits as natural data structure to save local variables.
{
var a = 1;
    {
    var b = 2;
        {
        var c = 3
        }
    }
}
stack: a
stack: b    a
stack  c    b   a
stack: b    a
stack  a
To resolve the local variables, we will emit a bytecode instruction with operand the position in the stack of the local variables.
 */
struct Local<'a> {
    name: Token<'a>,
    depth: Option<usize>
}

impl<'a> Local<'a> {
    fn new(name: Token<'a>, depth: Option<usize>) -> Self {
        Local {
            name,
            depth
        }
    }
}

struct Compiler<'a> {
    locals: Vec<Local<'a>>,
    local_count: u8,
    scope_depth: usize // number of blocks surrounding the current code we are compiling.
}

impl<'a> Compiler<'a> {
    fn new() -> Self {
        Compiler {
            locals: Vec::new(),
            local_count: 0,
            scope_depth: 0
        }
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
    strings: HashMap<String, Rc<String>>,
    compiler: Box<Compiler<'a>>,
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
        rules_map.insert(TokenType::Identifier, ParseRule::new(Some(Parser::variable), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::String, ParseRule::new(Some(Parser::string), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Number, ParseRule::new(Some(Parser::number), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::And, ParseRule::new(None, Some(Parser::and), Precedence::PrecedenceAnd));
        rules_map.insert(TokenType::Class, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Else, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::False, ParseRule::new(Some(Parser::literal), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::For, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Fun, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::If, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Nil, ParseRule::new(Some(Parser::literal), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Or, ParseRule::new(None, Some(Parser::or), Precedence::PrecedenceOr));
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
            strings: HashMap::new(),
            compiler: Box::new(Compiler::new()),
            had_error: false,
            panic_mode: false
        }
    }
    
    pub fn compile(&mut self) -> bool {
        self.advance();
        while !self.match_token_type(TokenType::Eof) {
            self.declaration();
        }
        //self.expression();
        self.consume(TokenType::Eof, "Expect end of expression.");
        self.end_compiler();
        !self.had_error
    }

    fn declaration(&mut self) {
        if self.match_token_type(TokenType::Var) {
            self.var_declaration();
        } else {
            self.statement();
        }
        if self.panic_mode {
            self.synchronize();
        }
    }

    fn var_declaration(&mut self) {
        let global = self.parse_variable("Expect variable name");

        if self.match_token_type(TokenType::Equal) {
            self.expression();
        } else {
            self.emit_byte(OpCode::OpNil.to_u8());
        }
        self.consume(TokenType::Semicolon, "Expected ';' after variable declaration.");
        self.define_variable(global);
    }

    fn statement(&mut self) {
        if self.match_token_type(TokenType::Print) {
            self.print_statement();
        } else if self.match_token_type(TokenType::For) {
            self.for_statement();
        } else if self.match_token_type(TokenType::If) {
            self.if_statement();
        } else if self.match_token_type(TokenType::While) {
            self.while_statement();
        } else if self.match_token_type(TokenType::LeftBrace) {
            self.begin_scope();
            self.block();
            self.end_scope();
        } else {
            self.expression_statement();
        }
    }

    /*
    * How the if is implemented by the compiler?
    * Basically we have the instruction pointer (ip) which is used
    * to know which current instruction to execute in the bytecode.
    * All that control flow is about is to change the value of the instruction pointer
    * to point to the instruction to execute depending on the result of the control flow statement
    * (in this case of the if expression).
    * The overall branch logic for the if-else is the following:
    *
    * condition_expression
    * OP_JUMP_IF_FALSE ** --->
    * OP_POP
    * then_branch
    * OP_JUMP *** --->
    * ** --> OP_POP
    * else_branch
    *  *** ---> remaining program
    *
     */
    fn if_statement(&mut self) {
        // first the expression to evaluate is consumed and put on the top of the stack
        self.consume(TokenType::LeftParen, "Expect '(' after if.");
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after condition.");

        let then_jump = self.emit_jump(OpCode::OpJumpIfFalse.to_u8());
        /*
        * The if is a statement, as such it should leave 0 effects on the stack
        * hence, the if condition needs to be cleaned from the stack.
        * If the condition is true, it is popped right after is evaluated.
         */
        self.emit_byte(OpCode::OpPop.to_u8());
        self.statement();

        /*
        * In case the then_branch is executed and there is an else branch, it is required
        * to jump after the else branch, otherwise the result would be to execute the then_branch
        * because the if condition holds true, and then to execute also the else branch, which should instead
        * be skipped.
         */
        let else_jump = self.emit_jump(OpCode::OpJump.to_u8());

        self.patch_jump(then_jump);
        // if the if condition is false, it is popped right before the else.
        self.emit_byte(OpCode::OpPop.to_u8());

        if self.match_token_type(TokenType::Else) {
            self.statement();
        }
        // if there is no else statement, the jump will point to the next instruction after the then_branch
        self.patch_jump(else_jump);
    }

    /*
    * The logic behind the while loop is that after the expression, a OP_JUMP_IF_FALSE
    * is emitted which has patch_jump right after the body of the while, so that if
    * the expression of the while loop is false, we jump straight after it.
    * Is also required to jump back. To do that, the position in the bytecode just before
    * the while condition is evaluated is saved in the loop_start variable, which is then
    * used to emit the LOOP instruction via the emit_loop utility function.
    *
    * It is interesting how from the VM perspective a LOOP operation
    * or an IF operation are both just changing the value of the
    * instruction pointer conditionally
     */
    fn while_statement(&mut self) {
        let loop_start = self.current_chunk().get_code().len();
        // first the expression to evaluate is consumed and put on the top of the stack
        self.consume(TokenType::LeftParen, "Expect '(' after while.");
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after condition.");

        let exit_jump = self.emit_jump(OpCode::OpJumpIfFalse.to_u8());
        self.emit_byte(OpCode::OpPop.to_u8());

        self.statement();
        self.emit_loop(loop_start);

        self.patch_jump(exit_jump);
        self.emit_byte(OpCode::OpPop.to_u8());
    }

    /*
    * initializer clause
    * **** --> condition_expression
    *
    * OP_JUMP_IF_FALSE ** --->
    * OP_POP
    * OP_JUMP *** -->
    *
    * ***** --> increment expression
    *
    * OP_POP
    * OP_LOOP **** -->
    *
    * *** --> body_statement
    *
    * OP_LOOP ***** -->
    * ** --> OP_POP
    *
     */
    fn for_statement(&mut self) {
        // if in the for variables are declared they need to be in their scope.
        self.begin_scope();

        self.consume(TokenType::LeftParen, "Expect '(' after for.");
        // initializer
        if self.match_token_type(TokenType::Semicolon) {
            // do nothing
        } else if self.match_token_type(TokenType::Var) {
            self.var_declaration();
        } else {
            // expression_statement looks for semicolon (which here is needed)
            // and emits OP_POP which is also needed to not have the initializer leave anything on the stack
            self.expression_statement()
        }
        let mut loop_start = self.current_chunk().get_code().len();

        let mut exit_jump = -1;
        // if the condition is omitted we look for a semicolon, if there is no semicolon
        // it means that the condition is not omitted, hence there must be a self.expression()
        if !self.match_token_type(TokenType::Semicolon) {
            self.expression();
            self.consume(TokenType::Semicolon, "Expected ';' after loop condition");

            // if the condition is false we must jump after the for loop
            exit_jump = self.emit_jump(OpCode::OpJumpIfFalse.to_u8()) as i32;
            self.emit_byte(OpCode::OpPop.to_u8());
        }

        /*
        * Increment clause
        * The increment appears before the body of the loop, but executes after.
        * Since the bytecode compiler does a single pass over the code, the logic is
        * to jump over the increment, execute the body and then jump back at it before
        * the next iteration.
        * The logic is that we jump directly to the body of the for loop with body_jump
        * then the increment expression is evaluated, which puts the expression value (the increment)
        * on the top of the stack and then given that is needed only as side effect its value is popped.
        * Then the idea is to emit_loop() to jump back to the start of the next for loop iteration right before
        * the loop condition, but then change the loop_start value, which gives the offset of the ip
        * where the emit_loop() jumps, with the start of the increment.
        * This way when the emit_loop() is emitted after the body of the for loop, we jump to the
        * increment instead and execute it.
        *
         */
        if !self.match_token_type(TokenType::RightParen) { // increment clause is optional, if not there is the closing parenthesis
            let body_jump = self.emit_jump(OpCode::OpJump.to_u8()); // this is the OP_JUMP in the schema above jumping to the body statement
            let increment_start = self.current_chunk().get_code().len();
            self.expression();
            self.emit_byte(OpCode::OpPop.to_u8());
            self.consume(TokenType::RightParen, "Expect ')' after for clauses.");

            self.emit_loop(loop_start);
            loop_start = increment_start;
            self.patch_jump(body_jump);
        }

        self.statement();
        self.emit_loop(loop_start);

        if exit_jump != -1 {
            self.patch_jump(exit_jump as usize);
            self.emit_byte(OpCode::OpPop.to_u8());
        }

        self.end_scope();
    }

    fn block(&mut self) {
        // while the block is not closed (or EOF is reached, in which case there is a malformed block)
        // keep compiling the statements within the block.
        while !self.check(TokenType::RightBrace) && !self.check(TokenType::Eof) {
            self.declaration();
        }

        self.consume(TokenType::RightBrace, "Expect '}' after the block.")
    }

    /*
    An expression followed by the ';', it is used to use an expression where a statement
    is expected.
    The expression is evaluated and the result is discarded: OP_POP pops the top value of the stack
    the expression has produced.
     */
    fn expression_statement(&mut self) {
        self.expression();
        self.consume(TokenType::Semicolon, "Expected ';' after value.");
        self.emit_byte(OpCode::OpPop.to_u8());
    }

    fn print_statement(&mut self) {
        self.expression();
        self.consume(TokenType::Semicolon, "Expected ';' after value.");
        self.emit_byte(OpCode::OpPrint.to_u8());
    }

    fn parse_variable(&mut self, msg: &str) -> u8 {
        self.consume(TokenType::Identifier, msg);

        self.declare_variable();
        /*
        At runtime locals are not looked up by name, the variable name
        does not need to be stuffed in the constant table.
         */
        if self.compiler.scope_depth > 0 {
            return 0;
        }

        self.identifier_constant()
    }

    fn identifier_constant(&mut self) -> u8 {
        // String interning
        let lexeme = self.previous.get_lexeme();
        let interned_identifier = self.intern_string(lexeme);
        self.make_constant(Value::String(interned_identifier))
    }

    /*
    The bytecode interpreter loop in the VM needs access to global variables by name,
    storing it in the bytecode instructions is too expensive, therefore the variable name
    is saved in the constants of the chunk and the index in the constants vec is used to access it.
     */
    fn define_variable(&mut self, index: u8) {
        /*
        No need to create a local variable at runtime (emitting bytecode).
        The VM has already executed the code to initialize the variable and the
        value is on top of the stack:
          {
            var a = 1 + 2;
            var b = 4;
           }
         stack: OP_CONSTANT 1
         stack: OP_CONSTANT 1   2
         stack: OP_ADD      3 // can be used to define local var a
         stack: OP_CONSTANT 3   4 // can be used to define local var b
         */
        if self.compiler.scope_depth > 0 {
            self.mark_initialized();
            return
        }

        self.emit_bytes(OpCode::OpDefineGlobal.to_u8(), index)
    }

    fn mark_initialized(&mut self) {
        /* Once the variable has declared and its initializer defined, it becomes available for use (defined).
        To avoid cases like:
        {
          var a = "outer";
          {
            var a = a;
          }
        }
         */
        let index = self.compiler.local_count - 1;
        self.compiler.locals[index as usize].depth = Some(self.compiler.scope_depth);
    }

    fn declare_variable(&mut self) {
        // if global scope return
        // global variables are late bound: meaning
        // they are resolved at runtime.
        if self.compiler.scope_depth == 0 {
            return
        }

        let name = self.previous;
        for i in (0..self.compiler.local_count).rev() {
            let local_depth = self.compiler.locals[i as usize].depth;
            let local_name = self.compiler.locals[i as usize].name;
            if local_depth.is_some() && local_depth.expect("Variable has not been defined.") < self.compiler.scope_depth {
                break;
            }
            if name.get_lexeme() == local_name.get_lexeme() {
                self.error("Already a variable with this name in this scope.");
            }
        }
        self.add_local(name);
    }

    /*
    Add the local variable in the compiler list of variables for the current scope.
     */
    fn add_local(&mut self, name: Token<'a>) {
        // The instructions to work with local variables refer to them by the index in the locals
        // the index is compiled as a bytecode operand, therefore we cannot address more than what a u8
        // is able to encode.
        if self.compiler.local_count == u8::MAX {
            self.error("Too many local variables in the function.");
            return;
        }
        let local = Local::new(name, None);
        self.compiler.local_count += 1;
        self.compiler.locals.push(local);
    }

    /*
    Here the code connecting parsing utilities (advance(), consume(), error_at()..) with code generation
    utilities (emit_byte(), emit_bytes()...).
     */
    fn expression(&mut self) {
        // compile the lowest precedence expression, which in turn will compile also all the higher level ones contained within it.
        self.parse_precedence(Precedence::PrecedenceAssignment);
    }

    /*
    * In logical operators like AND, if the left expression
    * is false, there is no point in evaluating the right-hand side
    * and we should return immediately keeping on the stack the left-hand side
    * expression. In case instead is needed to evaluate the right-hand side
    * the left-hand side is discarded from the stack.
    *
    * left operand expression
    * OP_JUMP_IF_FALSE ** --->
    * OP_POP
    * right operand expression
    * ** ---> remaining program
    *
     */
    fn and(&mut self, can_assign: bool) {
        // here the left-hand side of the expression

        let end_jump = self.emit_jump(OpCode::OpJumpIfFalse.to_u8());

        self.emit_byte(OpCode::OpPop.to_u8());

        // right-hand side of the expression
        self.parse_precedence(Precedence::PrecedenceAnd);

        self.patch_jump(end_jump);
    }

    /*
    * Basically if the left-hand side expression is false,
    * we jump directly at the right-hand side expression
    * while if it is true, we jump to OP_JUMP which jumps
    * after the right-hand side expression because the OR will
    * be for sure true.
    *
    * left operand expression
    * OP_JUMP_IF_FALSE ** --->
    * OP_JUMP *** -->
    * --> ** right operand expression
    * *** ---> remaining program
    *
     */
    fn or(&mut self, can_assign: bool) {
        let else_jump = self.emit_jump(OpCode::OpJumpIfFalse.to_u8());
        let end_jump = self.emit_jump(OpCode::OpJump.to_u8());

        self.patch_jump(else_jump);
        self.emit_byte(OpCode::OpPop.to_u8());

        self.parse_precedence(Precedence::PrecedenceOr);
        self.patch_jump(end_jump);
    }

    fn unary(&mut self, can_assign: bool) {
        let token_type = self.previous.get_token_type();

        /*
        Note that first the expression gets compiled and its bytecode added in the stack,
        and only after that the negate instruction bytecode is added in the stack. This makes sense,
        first the operand is evaluated and then negated. The compiler parse the code as written by the user
        but then re-arranges it in the correct execution order.
         */
        self.parse_precedence(Precedence::PrecedenceUnary);

        match token_type {
            TokenType::Minus => self.emit_byte(OpCode::OpNegate.to_u8()),
            TokenType::Bang => self.emit_byte(OpCode::OpNot.to_u8()),
            _ => unreachable!()
        }
    }

    fn string(&mut self, can_assign: bool) {
        let lexeme = self.previous.get_lexeme();
        let interned_string = self.intern_string(&lexeme[1..lexeme.len() - 1]);
        self.emit_constant(Value::String(interned_string));
    }

    fn intern_string(&mut self, lexeme: &str) -> Rc<String> {
        if let Some(value) = self.strings.get(lexeme) {
            value.clone()
        } else {
            let value = Rc::new(lexeme.to_string());
            self.strings.insert(lexeme.to_string(), value.clone());
            value
        }
    }

    fn variable(&mut self, can_assign: bool) {
        self.named_variable(can_assign);
    }

    fn named_variable(&mut self, can_assign: bool) {
        let get_op: u8;
        let set_op: u8;

        let mut arg = self.resolve_local();

        if arg.is_some() {
            get_op = OpCode::OpGetLocal.to_u8();
            set_op = OpCode::OpSetLocal.to_u8();
        } else {
            arg = Some(self.identifier_constant());
            get_op = OpCode::OpGetGlobal.to_u8();
            set_op = OpCode::OpSetGlobal.to_u8();
        }

        /*
        The bytecode compiler is a single-pass compiler, meaning
        it reads the source code and immediately emits bytecode.
        In cases like: menu.branch(sunday).beverage = "something";
        we need to understand if the expression menu.branch(sunday).beverage
        refers to a getter or a setter. To do so, is sufficient to check that
        there is an equal sign '=' after the trailing identifier: 'beverage'.
         */
        let arg_value = arg.expect("No value resolved.");
        if can_assign && self.match_token_type(TokenType::Equal) {
            self.expression();
            self.emit_bytes(set_op, arg_value)
        } else {
            self.emit_bytes(get_op, arg_value);
        }
    }

    /*
    The trick is a 1 to 1 matching between the index of a local variable added in self.compiler.locals
    and the index of the value for that local variable in self.stack in the Vm struct.
    In fact, when a local variable is declared, we add it in the self.compiler.locals,
    the value for that local variable is in the stack according to this:
       {
        var a = 1 + 2;
        var b = 4;
       }
     stack: OP_CONSTANT 1
     stack: OP_CONSTANT 1   2
     stack: OP_ADD      3 // can be used to define local var a
     stack: OP_CONSTANT 3   4 // can be used to define local var b
    NOTE: The values on the stack needed to resolve an expression are pushed and popped in the expression lifetime, therefore they
    don't interfere with the index of the value for that variable in the stack.
     */
    fn resolve_local(&mut self) -> Option<u8> {
        for i in (0..self.compiler.local_count).rev() {
            if let Some(local) = self.compiler.locals.get(i as usize) {
                if local.name.get_lexeme() == self.previous.get_lexeme() {
                    if local.depth.is_none() {
                        self.error("Can't read local variable in its own initializer.");
                    }
                    return Some(i);
                }
            }
        }
        None
    }

    /*
    A grouping expression allows to insert a lower precedence expression where a higher precedence one is expected,
    but it does not have a runtime semantic meaning for the backend, therefore no bytecode is emitted for it, but only for
    the expression within it.
     */
    fn grouping(&mut self, can_assign: bool) {
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after expression");
    }

    fn number(&mut self, can_assign: bool) {
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
    fn binary(&mut self, can_assign: bool) {
        // at the time binary() is called, the infix operator has been consumed and is in the self.previous token
        let operator_type = self.previous.get_token_type();
        let parse_rule = self.get_rule(&operator_type);
        let precedence = parse_rule.get_precedence();
        self.parse_precedence(Precedence::next(precedence));
        match operator_type {
            // a != b is equal to !(a == b)
            TokenType::BangEqual => self.emit_bytes(OpCode::OpEqual.to_u8(), OpCode::OpNot.to_u8()),
            TokenType::EqualEqual => self.emit_byte(OpCode::OpEqual.to_u8()),
            TokenType::Greater => self.emit_byte(OpCode::OpGreater.to_u8()),
            TokenType::GreaterEqual => self.emit_bytes(OpCode::OpLess.to_u8(), OpCode::OpNot.to_u8()),
            TokenType::Less => self.emit_byte(OpCode::OpLess.to_u8()),
            TokenType::LessEqual => self.emit_bytes(OpCode::OpGreater.to_u8(), OpCode::OpNot.to_u8()),
            TokenType::Plus => self.emit_byte(OpCode::OpAdd.to_u8()),
            TokenType::Minus => self.emit_byte(OpCode::OpSubtract.to_u8()),
            TokenType::Star => self.emit_byte(OpCode::OpMultiply.to_u8()),
            TokenType::Slash => self.emit_byte(OpCode::OpDivide.to_u8()),
            _ => unreachable!()
        }
    }

    fn literal(&mut self, can_assign: bool) {
        match self.previous.get_token_type() {
            TokenType::False => self.emit_byte(OpCode::OpFalse.to_u8()),
            TokenType::True => self.emit_byte(OpCode::OpTrue.to_u8()),
            TokenType::Nil => self.emit_byte(OpCode::OpNil.to_u8()),
            _ => unreachable!()
        }
    }

    fn get_rule(&mut self, operator_type: &TokenType) -> &ParseRule<'a> {
        &self.rules[operator_type]
    }

    fn begin_scope(&mut self) {
        self.compiler.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.compiler.scope_depth -= 1;
        while self.compiler.local_count > 0 {
            let local = &self.compiler.locals[(self.compiler.local_count - 1) as usize];
            if local.depth.expect("Local depth is not initialized.") <= self.compiler.scope_depth {
                break;
            }
            // local variables values occupy slots in the stack so when it goes out of scope the value is popped
            self.emit_byte(OpCode::OpPop.to_u8());
            self.compiler.local_count -= 1;
            self.compiler.locals.pop();
        }
    }

    /*
    * The OP_JUMP_IF_FALSE bytecode instruction takes also an operand: the offset
    * of how much to jump with the instruction pointer to start the instruction
    * following the then_branch (which in case the control flow expression if false is not executed).
    * However, at the time the OP_JUMP_IF_FALSE byte is emitted, the then branch has not been compiled
    * therefore, is not known how much to jump with the instruction pointer.
    * This is solved through the technique called 'backpatching'.
    * Basically, a placeholder is put in place of the operand following OP_JUMP_IF_FALSE which is then
    * filled with the correct value of how much to jump, once the then branch has been compiled.
    * Is required to remember where the OP_JUMP_IF_FALSE is in order to jump back to it and add the correct offset.
     */
    fn emit_jump(&mut self, bytecode: u8) -> usize {
        self.emit_byte(bytecode);
        // two bytes for the offset operand, e.g 16 bites: 65535 bytes we can jump to.
        self.emit_byte(0xff);
        self.emit_byte(0xff);
        self.current_chunk().get_code().len() - 2 // offset of first operand byte
    }

    fn patch_jump(&mut self, offset: usize) {
        /*
        * The idea is that when arriving here, the then_branch part has already been compiled
        * as for example in the case of the if_statement() this function is called after self.statement().
        * At this point to understand how much to jump (how many bytecodes the then_branch takes) you need
        * to subtract the offset passed, which is the offset of the OP_JUMP_IF_FALSE bytecode instruction in case of
        * the if minus 2 because after the OP_JUMP_IF_FALSE there are two bytes to make room for the operand
        * of how much to jump.
         */
        let jump = self.current_chunk().get_code().len() - offset - 2;

        match jump > u16::MAX as usize {
            true => self.error("Too much code to jump over."),
            false => {
                self.current_chunk().set_code(offset, ((jump >> 8) & 0xff) as u8);
                self.current_chunk().set_code(offset + 1, (jump & 0xff) as u8);
            }
        }
    }

    fn emit_loop(&mut self, loop_start: usize) {
        self.emit_byte(OpCode::OpLoop.to_u8());

        let offset = self.current_chunk().get_code().len() - loop_start + 2;
        if offset as u16 > u16::MAX {
            self.error("Loop body too large.");
        }

        self.emit_byte(((offset >> 8) & 0xff) as u8);
        self.emit_byte((offset & 0xff) as u8);
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

        /*
        Assignment is the lowest precedence expression, therefore is allowed only when
        parsing assignment expressions or top-level expressions like expression statements.
        Otherwise, expressions like a * b = c + d would be parsed as a * (b = c + d), indeed:
        - First, parsePrecedence() parses a using the variable() prefix parser.
        - After that, it enters the infix parsing loop.
        - It reaches the * and calls binary().
        - That recursively calls parsePrecedence() to parse the right-hand operand.
        - That calls variable() again for parsing b.
        - Inside that call to variable(), it looks for a trailing =.
        - It sees one and thus parses the rest of the line as an assignment.
         */
        let can_assign = precedence <= PrecedenceAssignment;


        if let Some(prefix) = prefix_rule {
            prefix(self, can_assign);
        } else {
            self.error("Expect expression.");
            return;
        }

        /*
        The example made above: a * b = c + d is actually a syntax error, a * b is not a valid identifier,
        therefore such kind of assignment needs to be reported as an error.
        The following helps us understand if we tried to do something like that.
         */
        if can_assign && self.match_token_type(TokenType::Equal) {
            self.error("Invalid assignment target.");
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
            infix_rule(self, can_assign);
        }
    }

    // add the value in the constants table and emits a OP_CONSTANT instruction
    // to push the code in the stack at runtime.
    fn emit_constant(&mut self, value: Value) {
        let index = self.make_constant(value);
        self.emit_bytes(OpCode::OpConstant.to_u8(), index)
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
        self.emit_byte(OpCode::OpReturn.to_u8());
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

    fn match_token_type(&mut self, token_type: TokenType) -> bool {
        if !self.check(token_type) {
            return false;
        }
        self.advance();
        true
    }

    fn check(&self, token_type: TokenType) -> bool {
        self.current.get_token_type() == token_type
    }

    /*
    If in the previous statement a compile-time error has been hit, in the current one, everything is discarded
    until a statement boundary: The end of the statement (semicolon) or the start of the following statement (control flow or declaration)
     */
    fn synchronize(&mut self) {
        self.panic_mode = false;

        while self.current.get_token_type() != TokenType::Eof {
            if self.previous.get_token_type() == TokenType::Semicolon {
                return;
            }
            match self.current.get_token_type() {
                TokenType::Class |
                TokenType::Fun   |
                TokenType::Var   |
                TokenType::For   |
                TokenType::If    |
                TokenType::While |
                TokenType::Print |
                TokenType::Return => return,
                _ => ()
            }
            self.advance();
        }
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
            TokenType::Eof => eprint!(" at end"),
            TokenType::Error => (),
            _ => eprint!("at {}", token.get_lexeme())
        }
        eprint!(": {}", message);
        self.had_error = true;
    }
}