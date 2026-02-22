use crate::chunk::{Chunk, OpCode};
use crate::compiler::Precedence::PrecedenceAssignment;
use crate::heap::{Heap, HeapObj, ObjRef};
use crate::object::FunctionType::{TypeFunction, TypeInitializer, TypeMethod};
use crate::object::{FunctionType, ObjFunction, CompilerUpvalue};
use crate::scanner::{Scanner, Token, TokenType};
use crate::value::Value;
use crate::value::Value::Obj;
use std::collections::HashMap;
use std::cell::RefCell;
use std::mem::replace;

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
    depth: Option<usize>,
    /*
    *
    * It is needed so that a certain compiler knows if it's variables are
    * captured by some closures in inner functions. If that is the case
    * this field will be useful to understand when a variable goes out of scope
    * (no more referenced) if only removing it from the stack (OP_POP) or instead
    * moving it to the heap to make it available for the closures when they need it.
    *
     */
    is_captured: bool
}

impl<'a> Local<'a> {
    fn new(name: Token<'a>, depth: Option<usize>) -> Self {
        Local {
            name,
            depth,
            is_captured: false
        }
    }
}

/*
* Tracks compile-time state for the class currently being compiled.
* A new ClassCompiler is pushed onto Parser::class_compilers when entering a class body
* and popped when leaving it. The vec therefore acts as a stack that mirrors the nesting
* of class declarations in the source.
*
* Having at least one entry in the stack means the compiler is currently inside a class
* body, which is used to validate that 'this' and (later) 'super' are only used where
* they are legal.
*
* Will hold has_superclass: bool once inheritance is implemented (Chapter 29).
*/
struct ClassCompiler {
    // True if the class being compiled has a superclass (`class Foo < Bar`).
    // Used to validate `super` usage and to know whether to close the `super` scope.
    has_superclass: bool
}

struct Compiler<'a> {
    /*
    * In order to allow having each function with its own chunk, the top-level program itself
    * is a function. The VM can run by always calling a function.
     */
    function: Option<ObjFunction>,
    type_fn: FunctionType,
    locals: Vec<Local<'a>>,
    local_count: u8,
    scope_depth: usize, // number of blocks surrounding the current code we are compiling.
    enclosing: Option<Box<RefCell<Compiler<'a>>>>,
    upvalues: Vec<CompilerUpvalue>
}

impl<'a> Compiler<'a> {
    fn new() -> Self {
        let mut locals = Vec::new();
        // the first slot of locals is assigned to the VM
        locals.push(
            Local {
                name: Token::new(""),
                depth: Some(0),
                is_captured: false
            }
        );

        Compiler {
            /*
            * A function object (runtime representation of a function)
            * is created in the compiler (hence during compilation)
            * because function represent something in between teh two steps,
            * like a string.
             */
            function: Some(ObjFunction::new()),
            type_fn: FunctionType::TypeScript,
            locals,
            local_count: 1,
            scope_depth: 0,
            enclosing: None,
            upvalues: Vec::new()
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
    fn resolve_local(&self, name: &str) -> Result<Option<u8>, ()> {
        for i in (0..self.local_count).rev() {
            if let Some(local) = self.locals.get(i as usize)
                && local.name.get_lexeme() == name {
                    if local.depth.is_none() {
                        return Err(()); // signal error: can't read in own initializer
                    }
                    return Ok(Some(i));
                }
        }
        Ok(None)
    }

    fn add_upvalue(&mut self, index: u8, is_local: bool) -> Option<u8> {
        let function = self.function.as_mut()?;
        let upvalue_count = function.upvalue_count;
        /*
        * If the function already has an upvalue for that variable
        * because it refers to that variable multiple times, the index in the upvalues array
        * is directly returned.
        * Otherwise, it that's not the case it means that the upvalue is new and is added in the array
         */
        for i in 0..upvalue_count {
            let upvalue = &self.upvalues[i];
            if upvalue.index == index && upvalue.is_local == is_local {
                return Some(i as u8);
            }
        }
        self.upvalues.push(CompilerUpvalue::new(index, is_local));
        function.upvalue_count += 1;
        Some(upvalue_count as u8)
    }

    /*
    * Upvalues is the mechanism used to allow a function to refer to variables
    * which are local to outer (previous) functions.
    * Indeed, one common strategy is to move to the heap closed-over local variables
    * so that later functions who need those can find them in the heap.
    * However, the compiler implemented is a single pass compiler, therefore there is
    * no way for the compiler to go back to already compiled variables to move to the heap
    * when it becomes aware that these variables are closed-over.
    * Every closure has an array of upvalues assigned.
    * Such array captures the local variables of other functions on which the closure (function)
    * closes over.
    * In particular the upvalue points to the location on the stack where such variable lives.
    *
    *                    ----------------> ObjFunction (chunk, constants)
    * ObjClosure (function, upvalues)
    *                               -----------------> Upvalue array ----> ObjUpvalue (location)
    *                              < --------------------------------------------------------
    *                             3     <fn>
    *
    * Given the following example:
    * {
    *   var a = 3;
    *   fun f() {
    *     print(a)
    *   }
    * }
    *
    * Question: The approach of giving to the functions a relative stack slot offset of the variables in outer
    * functions does not work. This is because when the enclosing function does not execute anymore (returns) its
    * stack slots are popped from the stack, therefore these offsets are meaningless.
    * One could argue, what is the difference with upvalues then, they also take an index which is the position
    * of the closed over variables on the stack.
    * The difference is that this is only the initial state. When the enclosing function is about to return, the VM
    * copies the value from the stack into the upvalue object itself (on the heap).
    * The upvalue is just an indirection layer.
     */
    fn resolve_upvalues(&mut self, name: &str) -> Result<Option<u8>, ()> {
        if self.enclosing.is_none() {
            return Ok(None);
        }

        let local_result = {
            let enclosing = self.enclosing.as_ref().unwrap();
            let enclosing_ref = enclosing.borrow();
            enclosing_ref.resolve_local(name)
        };

        match local_result {
            Ok(Some(index)) => {
                let enclosing = self.enclosing.as_ref().unwrap();
                enclosing.borrow_mut().locals[index as usize].is_captured = true;
                Ok(self.add_upvalue(index, true))
            }
            /*
            *
            * Why a recursive case is needed?
            * When a function closes over a variable not in the
            * immediately outer function but in functions in outer levels,
            * the closed over variable is removed from the stack before the
            * upvalue is able to capture it. The idea then is to create a chain
            * of upvalues until the upvalue in the function before the closed-over variable.
            * Meaning, given this example:
            * f() {
            *   var x = ...
            *   g() {
            *      h() {
            *         use x
            *      }
            *   }
            * }
            * Then h(upvalue) ---> g(upvalue) ----> f(var x)
            * And h reaches var x via the chain of upvalues.
            *
             */
            Ok(None) => {
                let upvalue = {
                    let enclosing = self.enclosing.as_ref().unwrap();
                    let mut enclosing_mut = enclosing.borrow_mut();
                    enclosing_mut.resolve_upvalues(name)?
                };
                match upvalue {
                    Some(index) => Ok(self.add_upvalue(index, false)),
                    None => Ok(None)
                }
            }
            Err(()) => Err(())
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
    rules: HashMap<TokenType, ParseRule<'a>>,
    compiler: Box<Compiler<'a>>,
    heap: &'a mut Heap,
    had_error: bool,
    panic_mode: bool,
    // Stack of ClassCompilers, one per currently-open class body. Non-empty iff the
    // compiler is currently inside a class declaration.
    class_compilers: Vec<ClassCompiler>
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str, heap: &'a mut Heap) -> Self {
        let mut rules_map = HashMap::new();
        rules_map.insert(TokenType::LeftParen, ParseRule::new(Some(Parser::grouping), Some(Parser::call), Precedence::PrecedenceCall));
        rules_map.insert(TokenType::RightParen, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::LeftBrace, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::RightBrace, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Comma, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Dot, ParseRule::new(None, Some(Parser::dot), Precedence::PrecedenceCall));
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
        rules_map.insert(TokenType::Super, ParseRule::new(Some(Parser::super_), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::This, ParseRule::new(Some(Parser::this), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::True, ParseRule::new(Some(Parser::literal), None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Var, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::While, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Error, ParseRule::new(None, None, Precedence::PrecedenceNone));
        rules_map.insert(TokenType::Eof, ParseRule::new(None, None, Precedence::PrecedenceNone));

        Parser {
            scanner: Scanner::new(source),
            current: Token::new("current"),
            previous: Token::new("previous"),
            rules: rules_map,
            compiler: Box::new(Compiler::new()),
            heap,
            had_error: false,
            panic_mode: false,
            class_compilers: Vec::new()
        }
    }
    
    pub fn compile(&mut self) -> Option<ObjFunction> {
        self.advance();
        while !self.match_token_type(TokenType::Eof) {
            self.declaration();
        }
        self.consume(TokenType::Eof, "Expect end of expression.");
        if self.had_error {
            None
        } else {
            Some(self.end_compiler())
        }
    }

    fn declaration(&mut self) {
        if self.match_token_type(TokenType::Var) {
            self.var_declaration();
        } else if self.match_token_type(TokenType::Class) {
            self.class_declaration();
        } else if self.match_token_type(TokenType::Fun) {
            self.fun_declaration();
        } else {
            self.statement();
        }
        if self.panic_mode {
            self.synchronize();
        }
    }

    fn class_declaration(&mut self) {
        // Push a ClassCompiler so that 'this' (and later 'super') know they are inside
        // a valid class body for the duration of compiling this class's methods.
        self.class_compilers.push(ClassCompiler { has_superclass: false });
        self.consume(TokenType::Identifier, "Expect class name.");

        // save class name
        let name = self.previous.get_lexeme();

        // the class name identifier is added to the surrounding function's constant table
        // indeed printing a class means printing its name therefore the compilers needs to put this name
        // somewhere for the runtime to find it
        let name_constant = self.identifier_constant();

        // the class name is also needed to associate the class to a variable of the same name
        self.declare_variable();

        // emit instruction to create the class object at runtime
        self.emit_bytes(OpCode::OpClass.to_u8(), name_constant);

        /*
        *
        * Above the class variable has been declared, however declaring a variable adds it to the scope
        * but it needs to be defined to be used.
        * For classes the variable is defined BEFORE the body of the class is compiled.
        * In this way the user can refer to the containing class inside the bodies of its (the class)
        * own method.
        *
         */
        self.define_variable(name_constant);

        /*
        * The class name has been compiled.
        * We encounter a '<' , the superclass token is also consumed.
        * Then self.variable() is called, which takes the previously consumed token
        * treats it as a variable reference and emits code to load the variable value:
        * basically it takes the superclass by name and loads it onto the stack.
        * self.named_variable() loads the subclass to do the inheriting on the stack:
        * OP_INHERIT works like OP_METHOD -> it modifies a class in this case to make it
        * inherit from the superclass.
        *
         */
        if self.match_token_type(TokenType::Less) {
            self.consume(TokenType::Identifier, "Expect superclass name");

            // Pushes the superclass value onto the stack by resolving it as a variable.
            self.variable(false);
            let superclass_name = self.previous.get_lexeme();

            if name == superclass_name {
                self.error("A class cannot inherit from itself")
            }

            /*
            * Open a new scope and declare 'super' as a local variable.
            * add_local() does not push anything — it simply claims the stack slot that
            * variable(false) just filled (the superclass value) and names it "super".
            * mark_initialized() immediately marks it usable (depth = Some(scope_depth))
            * so the compiler does not treat it as an uninitialized variable.
            *
            * Any method inside this class that references 'super' will have the compiler
            * resolve it through the normal upvalue mechanism: resolve_upvalues() walks the
            * enclosing compiler chain, finds 'super' as a local here, and captures it.
            * At runtime, each closure gets a copy of the ObjRef to the superclass via its
            * upvalue array, so 'super' remains accessible even after the class declaration
            * has finished executing.
            */
            self.begin_scope();
            self.add_local(Token::new("super"));
            self.mark_initialized();

            self.named_variable(name, false); // loads the subclass for OP_INHERIT
            self.emit_byte(OpCode::OpInherit.to_u8());
            self.class_compilers.last_mut().unwrap().has_superclass = true;
        }

        // necessary to put the name of the class at the top of the stack before
        // starting the method declaration, otherwise the methods are not able to locate the class
        // when binding to it: it could be on the stack somewhere as local variable or on the globals
        // in case is a top-level declaration class.
        // basically while compiling the methods, these will be on the top of the stack right on top of the
        // class name
        self.named_variable(name, false);

        self.consume(TokenType::LeftBrace, "Expect '{' before class body");

        /*
        * A class could contain an arbitrarily big number of methods.
        * If the compiler emitted 1 single OP_CLASS instruction with all
        * the bytecode for the methods the runtime would have a lot of work to do
        * for class declaration.
        * Instead, the OP_CLASS instruction declares an empty class.
        * Then for each method, an OP_METHOD instruction is emitted, which populates
        * that class with a new method.
        * The result is a full complete class with all its methods.
         */
        while !self.check(TokenType::RightBrace) && !self.check(TokenType::Eof) {
            self.method();
        }

        self.consume(TokenType::RightBrace, "Expect '}', after class body");

        // Close the 'super' scope if one was opened, emitting OpCloseUpvalue so that
        // any method closure that captured 'super' as an upvalue keeps a valid reference
        // to the superclass after the stack slot is gone.
        if self.class_compilers.last().map(|cc| cc.has_superclass).unwrap_or(false) {
            self.end_scope();
        }

        // Class body is fully compiled; restore the enclosing class context (or none).
        self.class_compilers.pop();

        // The name of the class is no longer needed on the stack and can be popped
        self.emit_byte(OpCode::OpPop.to_u8());
    }


    /*
    * A new method definition requires three things:
    *   - Name of the method
    *   - Closure for the method body
    *   - Class to bind the method to
    *
     */
    fn method(&mut self) {
        self.consume(TokenType::Identifier, "Expect method name.");
        // the compiler adds the method name to the constant table and get back the index in the table
        // then it emits the OP_METHOD instruction passing as byte argument the index of the method name in the constant
        // table
        let constant = self.identifier_constant();

        /*
        * function(): helper wrote for compiling function declarations.
        * That utility function compiles the subsequent parameter list and function body.
        * Then it emits the code to create an ObjClosure and leave it on top of the stack.
        * At runtime, the VM will find the closure there
         */
        let mut function_type = TypeMethod;

        /*
        * We want to make sure that when the user defines the init() initializer method,
        * this method returns automatically "this" (the instance) without the need of the user to
        * remember to define "return this" therefore we need to modify the logic that normally returns
        * the default "nil" to return "this" in this case.
         */
        if self.previous.get_lexeme() == "init" {
            function_type = TypeInitializer;
        }
        self.function(function_type);
        self.emit_bytes(OpCode::OpMethod.to_u8(), constant);
    }

    /*
    * A function declaration is nothing else than a variable declaration
    * Depending on if the function is declared at top-scope or in a local scope
    * it will originate into a global or local variable.
    * Differently from variables which gets defined in two steps, to avoid accessing
    * a variable not yet initialized, with function this is legit, to allow for recursion.
     */
    fn fun_declaration(&mut self) {
        let global = self.parse_variable("Expect function name");
        // the function is defined as initialized as soon as the name is compiled
        // so that it can already be referenced without causing errors
        self.mark_initialized();

        // helper function to compile the body of the function
        self.function(TypeFunction);

        // now the body of the function is on top of the stack
        // therefore the previously defined variable is bounded to
        // its body
        self.define_variable(global);
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
        } else if self.match_token_type(TokenType::Return) {
            self.return_statement();
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

    fn return_statement(&mut self) {
        if self.compiler.type_fn == FunctionType::TypeScript {
            self.error("Can't return from top-level code.");
        }
        if self.match_token_type(TokenType::Semicolon) {
            self.emit_return();
        } else {
            if self.compiler.type_fn == FunctionType::TypeInitializer {
                self.error("Can't return a value from an initializer.");
                // we still continue compiling to avoid the compiler to get confused
                // by the trailing expression starting giving unrelated errors.
            }
            self.expression();
            self.consume(TokenType::Semicolon, "Expect ';' after return.");
            self.emit_byte(OpCode::OpReturn.to_u8());
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

    fn function(&mut self, function_type: FunctionType) {
        /*
        * A compiler stores information such as which stack slots
        * are owned by which local variables.
        * Such information is function-specific, given the nesting of
        * functions is required to nest compilers as well, assigning to each
        * function a compiler, whose "enclosing" compiler is saved in the current
        * compiler self.enclosing field.
        * As the body of the function is compiled, the bytecode is emitted and written
        * into the chunk owned by the function that the current compiler is assigned to.
         */
        let mut compiler = Compiler::new();
        compiler.type_fn = function_type;
        /*
        * "this" is treated as a local variable.
        * Closures inside a method that references this will capture the receiver in an upvalue.
        * in the this() function self.variable(false) is used.
        * self.variable(boolean) compiles identifier expressions as variable accesses.
        * The boolean parameter is needed to tell the function if it should look for a following '='
        * and parse a setter, we don't want that with "this".
        * self.variable does not care that "this" is a special reserved word and treats it like a normal
        * variable which should be looked up using the usual scope resolution mechanism.
        * Until captured by closures, every local variable is stored on the VM stack.
        * We reserved the slot zero in the function stack window with an empty local variable.
        * For function calls that slots end up holding the function being called, but since the name is
        * empty, the function body never access that local variable.
        * For methods, we will use that slot to store the receiver, e.g the instance that is pointed by "this"
        *
        *
        * To understand why it works:
        * The key is understanding what slot 0 actually contains at runtime when a method is called.
        *
        * When the VM calls a method, it sets up the CallFrame such that frame.slots points to the position on the stack where the receiver (the instance) sits.
        * That is slot 0 of the call frame. This is the same convention used for
        * regular functions, where slot 0 holds the closure itself — for methods, slot 0 holds the instance instead.
        *
        * So at runtime, the instance is physically sitting at slot 0 of the method's call frame.
        *
        * At compile time, when the compiler sees this inside a method body, it calls resolve_local("this").
        * It walks compiler.locals looking for a local whose name matches "this". It finds it at index 0. So it emits OpGetLocal 0.
        *
        *
        * At runtime, OpGetLocal 0 reads stack[frame.slots + 0] — which is exactly the receiver that the VM placed there when it set up the call frame.
         */
        if function_type != TypeFunction && function_type != FunctionType::TypeScript {
            compiler.locals[0].name = Token::new("this");
        }
        let enclosing_compiler = replace(self.compiler.as_mut(), compiler);
        self.compiler.enclosing = Some(Box::new(RefCell::new(enclosing_compiler)));
        if function_type != FunctionType::TypeScript {
            let name = self.heap.intern_string(self.previous.get_lexeme().to_string());
            self.compiler.function.as_mut().expect("Expect current function").name = Some(name);
        }
        self.begin_scope();

        self.consume(TokenType::LeftParen, "Expect '(' after function name.");
        if !self.check(TokenType::RightParen) {
            loop {
                let current_function = self.compiler.function.as_mut().expect("Expect function");
                current_function.arity += 1;
                if current_function.arity > 255 {
                    self.error_at_current("Cannot have more than 255 parameters.");
                }
                let constant = self.parse_variable("Expect parameter name.");
                self.define_variable(constant);
                if !self.match_token_type(TokenType::Comma) {
                    break
                }
            }
        }
        self.consume(TokenType::RightParen, "Expect ')' after function name.");
        self.consume(TokenType::LeftBrace, "Expect '{' before function body.");

        self.block();
        let upvalues = self.compiler.upvalues.clone();
        let function = self.end_compiler();
        let upvalue_count = function.upvalue_count;
        let obj_ref = self.heap.alloc(HeapObj::Function(function));
        let constant = self.make_constant(Obj(obj_ref));
        //self.emit_bytes(OpCode::OpConstant.to_u8(), constant)
        self.emit_bytes(OpCode::OpClosure.to_u8(), constant);
        for i in 0..upvalue_count {
            if upvalues[i].is_local {
                self.emit_byte(1);
            } else {
                self.emit_byte(0);
            }
            self.emit_byte(upvalues[i].index)
        }
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
        self.make_constant(Value::Obj(interned_identifier))
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
        // if it has been called from global environment just return, it is a global variable
        if self.compiler.scope_depth == 0 {
            return
        }
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
    * Once the left paren has been compiled,
    * the arguments of the function call are compiled
    * via the argument_list, which puts them in the stack
    * then it returns the number of arguments in order to
    * understand how many values to pick up from the stack.
    * Then it emits the OP_CALL which executes the body of the
    * function, using the parameters.
     */
    fn call(&mut self, can_assign: bool) {
        let arguments = self.argument_list();
        self.emit_bytes(OpCode::OpCall.to_u8(), arguments)
    }

    fn argument_list(&mut self) -> u8 {
        let mut arg_count: u8 = 0;
        if !self.check(TokenType::RightParen) {
            loop {
                self.expression();
                /*
                * Now is clear why it is required to have max 255 arguments:
                * having the arg_count as an u8 (1 byte) is not possible to represent more.
                 */
                if arg_count == 255 {
                    self.error("Cannot have more than 255 arguments.");
                }
                arg_count += 1;
                if !self.match_token_type(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, "Expect ')' after function arguments.");
        arg_count
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
        self.emit_constant(Value::Obj(interned_string));
    }

    fn intern_string(&mut self, lexeme: &str) -> ObjRef {
        self.heap.intern_string(lexeme.to_string())
    }

    fn variable(&mut self, can_assign: bool) {
        let variable_name = self.previous.get_lexeme();
        self.named_variable(variable_name, can_assign);
    }

    fn named_variable(&mut self, name: &str, can_assign: bool) {
        let get_op: u8;
        let set_op: u8;

        //let name = self.previous.get_lexeme().to_string();
        let mut arg = match self.compiler.resolve_local(&name) {
            Ok(result) => result,
            Err(()) => {
                self.error("Can't read local variable in its own initializer.");
                None
            }
        };

        if arg.is_some() {
            get_op = OpCode::OpGetLocal.to_u8();
            set_op = OpCode::OpSetLocal.to_u8();
        } else {
            // if the local variable is not found in the inner scope
            // is searched in all the outer scoped functions
            // only in the last case it looks in the global scope
            arg = match self.compiler.resolve_upvalues(&name) {
                Ok(result) => result,
                Err(()) => {
                    self.error("Can't read local variable in its own initializer.");
                    None
                }
            };
            if arg.is_some() {
                get_op = OpCode::OpGetUpValue.to_u8();
                set_op = OpCode::OpSetUpValue.to_u8();
            } else {
                arg = Some(self.identifier_constant());
                get_op = OpCode::OpGetGlobal.to_u8();
                set_op = OpCode::OpSetGlobal.to_u8();
            }
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

    fn dot(&mut self, can_assign: bool) {
        self.consume(TokenType::Identifier, "Expect property name after '.'.");
        // after the dot the parser expects a property name
        // that token is load in the constant table as a string to make it available at runtime
        let name = self.identifier_constant();

        /*
          *  In Pratt parsing, can_assign is true only when the current expression is being parsed at a precedence level at or below assignment (PREC_ASSIGNMENT)
          *  Assignment is the lowest-precedence operation. If you're currently parsing at a higher precedence, you're inside a sub-expression where = cannot possibly be a valid assignment operator — it would violate the grammar.
          *  So can_assign = false is just encoding that grammatical constraint.
          *  It gets passed down through the parse rule calls.
          *
          *  For a + b.c = 3:
          *
          *  1. The parser starts parsing a + b.c = 3 at PREC_ASSIGNMENT level, so can_assign = true initially.
          *  2. It parses a, then sees +. Since + has higher precedence than assignment, it recurses to parse the right-hand side at PREC_TERM or higher — meaning can_assign is now false for anything parsed on the right of +.
          *  3. It parses b, then sees ., which triggers dot as an infix rule — with can_assign = false.
          *  4. Inside dot, can_assign && self.match_token_type(Equal) is false, so it does not consume the = and emit OpSetProperty.
          *  5. The = 3 is then left unconsumed, causing a compile error.
          *
          *  Without the can_assign guard, dot would see the =, consume it, emit OpSetProperty, and silently compile the invalid assignment as if it were valid.
         */
        if can_assign && self.match_token_type(TokenType::Equal) {
            self.expression();
            self.emit_bytes(OpCode::OpSetProperty.to_u8(), name);

        } else if self.match_token_type(TokenType::LeftParen) {
            /*
            *  When a method like obj.method(arg) is called, the VM does two separate operations:
            * 1. OpGetProperty — looks up method on obj, creates an ObjBoundMethod pairing the closure with the receiver, pops the instance, and pushes the bound method onto the stack.
            * 2. OpCall — pops the bound method, sets up a CallFrame, and calls the closure.
            *
            * The ObjBoundMethod is heap-allocated just to be immediately consumed and discarded by OpCall. It exists for only one instruction's lifetime. This means:
            * - A heap allocation is made and then thrown away
            * - The GC has to track this short-lived object
            * - Two dispatch iterations through the VM loop instead of one
            *
            * OpInvoke fuses these two steps into a single instruction that takes both the method name and the argument count as operands.
            * At runtime it looks up the method on the instance's class and calls the closure directly, skipping the ObjBoundMethod allocation entirely.
            * The optimization is only valid when the compiler sees the pattern expr.name(args) as a single syntactic unit —
            * If the method is accessed without an immediate call (e.g. var f = obj.method), OpGetProperty is still used as before since a bound method is needed in that case.
            *
             */
            let arg_count = self.argument_list();
            self.emit_bytes(OpCode::OpInvoke.to_u8(), name); // name is the index in the constant table
            self.emit_byte(arg_count);

        } else {
            self.emit_bytes(OpCode::OpGetProperty.to_u8(), name);
        }
    }

    /*
    * 'this' is syntactic sugar for the receiver local at slot 0 of the current method's
    * call frame. The compiler resolves it exactly like any other local variable whose
    * name happens to be "this" (placed in locals[0] by function() for TypeMethod and
    * TypeInitializer).
    *
    * Guarded by class_compilers: if we are not inside a class body there is no slot 0
    * receiver and 'this' is illegal. Without this guard 'this' would fall through to a
    * global lookup and produce a confusing runtime error instead of a clear compile error.
    */
    fn this(&mut self, can_assign: bool) {
        if self.class_compilers.is_empty() {
            self.error("Can't use 'this' outside of a class.");
            return;
        }
        self.variable(false);
    }

    /*
    * Compiles a `super.method` or `super.method(args)` expression.
    *
    * Stack layout produced before the opcode is emitted:
    *   [ this (receiver) ]  [ args... ]  [ super (superclass) ]
    *
    * 'this' is pushed first so the method can be bound to the correct instance.
    * 'super' is pushed last so the VM knows which class to search for the method.
    *
    * If a '(' immediately follows the method name, the OpSuperInvoke optimization is
    * used: arguments are compiled between 'this' and 'super', and the VM performs the
    * lookup and call in a single instruction without allocating a bound method object.
    * Otherwise, OpGetSuper looks up the method in the superclass and leaves a bound
    * method on the stack for a subsequent OpCall.
    */
    fn super_(&mut self, can_assign: bool) {
        if self.class_compilers.is_empty() {
            self.error("Can't use 'super' outside of a class.");
        } else if !self.class_compilers.last().unwrap().has_superclass {
            self.error("Can't use 'super' in a class with no superclass.");
        }

        self.consume(TokenType::Dot, "Expect '.' after 'super'.");
        self.consume(TokenType::Identifier, "Expect superclass method name.");
        // intern the method name and store it in the constant pool
        let method_name = self.identifier_constant();

        // 'this' is the instance the looked-up method will be bound to
        self.named_variable("this", false);

        if self.match_token_type(TokenType::LeftParen) {
            // OpSuperInvoke path: compile args, then load 'super', emit combined op
            let arg_count = self.argument_list();
            self.named_variable("super", false);
            self.emit_bytes(OpCode::OpSuperInvoke.to_u8(), method_name);
            self.emit_byte(arg_count);
        } else {
            // OpGetSuper path: load 'super', emit lookup — leaves a bound method on stack
            self.named_variable("super", false);
            self.emit_bytes(OpCode::OpGetSuper.to_u8(), method_name);
        }
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
            /*
            *
            * If a variable in the function is captured by some other function
            * then when is going out of scope, it cannot be simply removed from the
            * stack anymore, otherwise the function closing over it won't be able to find it.
            * Instead, the variable needs to be moved from the stack to the heap.
            * The OP_CLOSE_UPVALUE opcode signals to the runtime exactly this.
            *
             */
            if self.compiler.locals[(self.compiler.local_count - 1) as usize].is_captured {
                self.emit_byte(OpCode::OpCloseUpValue.to_u8())
            } else {
                // local variables values occupy slots in the stack so when it goes out of scope the value is popped
                self.emit_byte(OpCode::OpPop.to_u8());
            }
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

      This correctly evaluates as 1 + (2 * 3) = 7

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

    fn end_compiler(&mut self) -> ObjFunction {
        #[cfg(feature = "debug-trace-execution")]
        {
            let function_name = match self.compiler.function.as_ref().and_then(|f| f.name) {
                Some(name) => &self.heap.get(name).as_string().chars,
                None => "script"
            };
            if !self.had_error {
                disassemble_chunk(self.current_chunk(), function_name, &self.heap);
            }
        }
        self.emit_return();
        let function = self.compiler.function.take().expect("Valid function.");
        if let Some(enclosing) = self.compiler.enclosing.take() {
            self.compiler = Box::new(enclosing.into_inner());
        }
        function
    }

    fn emit_return(&mut self) {
        if self.compiler.type_fn == FunctionType::TypeInitializer {
            self.emit_bytes(OpCode::OpGetLocal.to_u8(), 0);
        } else {
            self.emit_byte(OpCode::OpNil.to_u8());

        }
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
        // current chunk is always the chunk of the function that is being compiled
        &mut self.compiler.function.as_mut().expect("Valid function.").chunk
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