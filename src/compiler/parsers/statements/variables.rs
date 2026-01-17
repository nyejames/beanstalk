use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast::{ContextKind, ScopeContext};
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::build_ast::function_body_to_ast;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::field_access::parse_field_access;
use crate::compiler::parsers::statements::functions::{FunctionSignature, parse_function_call};
use crate::compiler::parsers::statements::structs::create_struct_definition;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, Token, TokenKind};
use crate::compiler::parsers::{
    ast_nodes::{NodeKind, Var},
    expressions::parse_expression::create_expression,
};
use crate::compiler::string_interning::{StringId, StringTable};
use crate::{ast_log, return_rule_error, return_syntax_error};
use std::collections::HashMap;

pub fn create_reference(
    token_stream: &mut FileTokens,
    reference_arg: &Var,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Move past the name
    token_stream.advance();

    match reference_arg.value.data_type {
        // Function Call
        DataType::Function(_, ref signature) => parse_function_call(
            token_stream,
            &reference_arg.id,
            context,
            signature,
            string_table,
        ),

        _ => {
            // This either becomes a reference or field access
            parse_field_access(token_stream, reference_arg, context, string_table)
        }
    }
}

// The standard declaration syntax.
// Parses any new variable, function or parameter.
// [name] [optional mutability '~'] [optional type] [assignment operator '='] [value]
enum DeclarationKind {
    Var {
        mutable: bool,
        type_declaration: Vec<Token>,
    },
    Function {
        receiver: Option<DataType>,
        signature: FunctionSignature,
    },
}
struct Declaration {
    name: StringId,
    directives: Vec<Token>,
    body: Vec<Token>,
    kind: DeclarationKind,
}

// Declarations vs Vars

// Declarations are not type-checked or folded, they just parse the structure of a declaration before lowering to an AST node.
// New Var takes a declaration and converts it into a fully parsed AstNode,
// it performs all the folding and type checking of the containing expression also.

// Declarations are used at the Header parsing stage,
// Var is used during AST creating when types and names must be known

impl Declaration {
    pub fn new_var(
        name: StringId,
        mutable: bool,
        type_declaration: Vec<Token>,
        rvalue: Vec<Token>,
        directives: Vec<Token>,
    ) -> Self {
        Self {
            name,
            directives,
            kind: DeclarationKind::Var {
                mutable,
                type_declaration,
            },
            body: rvalue,
        }
    }

    pub fn new_function(
        name: StringId,
        directives: Vec<Token>,
        signature: FunctionSignature,
        body: Vec<Token>,
    ) -> Self {
        Self {
            name,
            directives,
            kind: DeclarationKind::Function {
                receiver: None,
                signature,
            },
            body,
        }
    }

    pub fn new_method(
        receiver: DataType,
        name: StringId,
        directives: Vec<Token>,
        signature: FunctionSignature,
        body: Vec<Token>,
    ) -> Self {
        Self {
            name,
            directives,
            kind: DeclarationKind::Function {
                receiver: Some(receiver),
                signature,
            },
            body,
        }
    }
}

pub fn new_declaration(
    token_stream: &mut FileTokens,
    file_imports: &HashMap<StringId, InternedPath>,
    id: StringId,
    context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
    host_registry: &HostFunctionRegistry,
) -> Result<Declaration, CompilerError> {
    // Move past the name
    token_stream.advance();

    let mut mutable = false;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        mutable = true;
    };

    let mut data_type = Vec::new();

    match token_stream.current_token_kind() {
        // Go straight to the assignment
        TokenKind::Assign => {}

        TokenKind::TypeParameterBracket => {
            let empty_context = ScopeContext::new(
                ContextKind::Module,
                token_stream.src_path,
                &[],
                host_registry.to_owned(),
                Vec::new(),
            );

            let signature = FunctionSignature::new(token_stream, &empty_context, string_table)?;

            let mut scopes_opened = 1;
            let mut scopes_closed = 0;
            let mut function_body = Vec::new();

            // FunctionSignature::new leaves us at the first token of the function body
            // Don't advance before the first iteration
            while scopes_opened > scopes_closed {
                match token_stream.current_token_kind() {
                    TokenKind::End => {
                        scopes_closed += 1;
                        if scopes_opened > scopes_closed {
                            function_body.push(token_stream.tokens[token_stream.index].to_owned());
                        }
                    }

                    // Colons used in templates parse into a different token (EndTemplateHead),
                    // so there isn't any issue with templates creating a colon imbalance.
                    // But all features in the language MUST otherwise follow the rule that all colons are closed with semicolons.
                    // The only violations of this rule have to be parsed differently in the tokenizer,
                    // but it's better from a language design POV for colons to only mean one thing as much as possible anyway.
                    TokenKind::Colon => {
                        scopes_opened += 1;
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }

                    // Double colons need to be closed with semicolons also
                    TokenKind::DoubleColon => {
                        scopes_opened += 1;
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }

                    // Will now parse each symbol into a possible declaration at the header stage
                    // This is to avoid duplicating the logic for parsing declarations
                    // and do some parsing work ahead of the AST stage.
                    TokenKind::Symbol(name_id) => {
                        if let Some(path) = file_imports.get(name_id) {
                            dependencies.insert(path);
                        }
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }
                    _ => {
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }
                }
                token_stream.advance();
            }

            return Ok(Declaration {});
        }

        // Has a type declaration
        TokenKind::DatatypeInt => data_type = DataType::Int,
        TokenKind::DatatypeFloat => data_type = DataType::Float,
        TokenKind::DatatypeBool => data_type = DataType::Bool,
        TokenKind::DatatypeString => data_type = DataType::String,

        // Collection Type Declaration
        TokenKind::OpenCurly => {
            token_stream.advance();

            // Check if there is a type inside the curly braces
            data_type = match token_stream.current_token_kind().to_datatype() {
                Some(data_type) => DataType::Collection(Box::new(data_type), ownership.to_owned()),
                _ => DataType::Collection(Box::new(DataType::Inferred), Ownership::MutableOwned),
            };

            // Make sure there is a closing curly brace
            if token_stream.current_token_kind() != &TokenKind::CloseCurly {
                return_syntax_error!(
                    "Missing closing curly brace for collection type declaration",
                    token_stream.current_location().to_error_location(string_table), {
                        CompilationStage => "Variable Declaration",
                        PrimarySuggestion => "Add '}' to close the collection type declaration",
                        SuggestedInsertion => "}",
                    }
                )
            }
        }

        TokenKind::Newline => {
            data_type = DataType::Inferred;
            // Ignore
        }

        TokenKind::Colon => {
            let struct_def = create_struct_definition(token_stream, context, string_table)?;

            return Ok(Var {
                id,
                value: Expression::struct_definition(
                    struct_def,
                    token_stream.current_location(),
                    ownership,
                ),
            });
        }

        // SYNTAX ERRORS
        // Probably a missing reference or import
        TokenKind::Dot
        | TokenKind::AddAssign
        | TokenKind::SubtractAssign
        | TokenKind::DivideAssign
        | TokenKind::MultiplyAssign => {
            return_syntax_error!(
                format!(
                    "{} is undefined. Can't use {:?} after an undefined variable. Either define this variable first, import it or make sure its in scope.",
                    string_table.resolve(id),
                    token_stream.tokens[token_stream.index].kind
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Make sure to import or define this variable before using it.",
                }
            )
        }

        // Other kinds of syntax errors
        _ => {
            return_syntax_error!(
                format!(
                    "Invalid token: {:?} after new variable declaration. Expect a type or assignment operator.",
                    token_stream.tokens[token_stream.index].kind
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Use a type declaration (Int, String, etc.) or assignment operator '='",
                }
            )
        }
    };

    // Check for the assignment operator next
    // If this is parameters or a struct, then we can instead break out with a comma or struct close bracket
    token_stream.advance();

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        // If end of statement, then it's unassigned.
        // For the time being, this is a syntax error.
        // When the compiler becomes more sophisticated,
        // it will be possible to statically ensure there is an assignment on all future branches.

        // Struct bracket should only be hit here in the context of the end of some parameters
        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => {
            let var_name = string_table.resolve(id);
            return_rule_error!(
                format!("Variable '{}' must be initialized with a value", var_name),
                token_stream.current_location().to_error_location(string_table), {
                    // VariableName => var_name,
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Add '= value' after the variable declaration",
                }
            )
        }

        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected Token: {:?}. Are you trying to reference a variable that doesn't exist yet?",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Check that all referenced variables are declared before use",
                }
            )
        }
    }

    // The current token should be whatever is after the assignment operator

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away
    let parsed_expr = match token_stream.current_token_kind() {
        // Struct Definition
        // TokenKind::TypeParameterBracket => {
        //     // TODO
        // }
        TokenKind::OpenParenthesis => {
            token_stream.advance();
            create_expression(
                token_stream,
                context,
                &mut data_type,
                &ownership,
                true,
                string_table,
            )?
        }

        _ => create_expression(
            token_stream,
            context,
            &mut data_type,
            &ownership,
            false,
            string_table,
        )?,
    };

    ast_log!("Created new variable of type: {}", data_type);

    Ok(Var {
        id: id,
        value: parsed_expr,
    })
}

pub fn new_var(
    token_stream: &mut FileTokens,
    id: StringId,
    context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Var, CompilerError> {
    // Move past the name
    token_stream.advance();

    let mut ownership = Ownership::ImmutableOwned;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        ownership = Ownership::MutableOwned;
    };

    let mut data_type: DataType;

    match token_stream.current_token_kind() {
        // Go straight to the assignment
        TokenKind::Assign => {
            // Cringe Code
            // This whole function can be reworked to avoid this go_back() later.
            // For now, it's easy to read and parse this way while working on the specifics of the syntax
            token_stream.go_back();
            data_type = DataType::Inferred;
        }

        TokenKind::TypeParameterBracket => {
            let func_sig = FunctionSignature::new(token_stream, context, string_table)?;
            let func_context = context.new_child_function(id, func_sig.to_owned(), string_table);

            // TODO: fast check for function without signature
            // let context = context.new_child_function(name, &[]);
            // return Ok(Arg {
            //     name: name.to_owned(),
            //     value: Expression::function_without_signature(
            //         new_ast(token_stream, context, false)?.ast,
            //         token_stream.current_location(),
            //     ),
            // });

            let function_body = function_body_to_ast(
                token_stream,
                func_context.to_owned(),
                warnings,
                string_table,
            )?;

            return Ok(Var {
                id,
                value: Expression::function(
                    None,
                    func_sig,
                    function_body,
                    token_stream.current_location(),
                ),
            });
        }

        // Has a type declaration
        TokenKind::DatatypeInt => data_type = DataType::Int,
        TokenKind::DatatypeFloat => data_type = DataType::Float,
        TokenKind::DatatypeBool => data_type = DataType::Bool,
        TokenKind::DatatypeString => data_type = DataType::String,

        // Collection Type Declaration
        TokenKind::OpenCurly => {
            token_stream.advance();

            // Check if there is a type inside the curly braces
            data_type = match token_stream.current_token_kind().to_datatype() {
                Some(data_type) => DataType::Collection(Box::new(data_type), ownership.to_owned()),
                _ => DataType::Collection(Box::new(DataType::Inferred), Ownership::MutableOwned),
            };

            // Make sure there is a closing curly brace
            if token_stream.current_token_kind() != &TokenKind::CloseCurly {
                return_syntax_error!(
                    "Missing closing curly brace for collection type declaration",
                    token_stream.current_location().to_error_location(string_table), {
                        CompilationStage => "Variable Declaration",
                        PrimarySuggestion => "Add '}' to close the collection type declaration",
                        SuggestedInsertion => "}",
                    }
                )
            }
        }

        TokenKind::Newline => {
            data_type = DataType::Inferred;
            // Ignore
        }

        TokenKind::Colon => {
            let struct_def = create_struct_definition(token_stream, context, string_table)?;

            return Ok(Var {
                id,
                value: Expression::struct_definition(
                    struct_def,
                    token_stream.current_location(),
                    ownership,
                ),
            });
        }

        // SYNTAX ERRORS
        // Probably a missing reference or import
        TokenKind::Dot
        | TokenKind::AddAssign
        | TokenKind::SubtractAssign
        | TokenKind::DivideAssign
        | TokenKind::MultiplyAssign => {
            return_syntax_error!(
                format!(
                    "{} is undefined. Can't use {:?} after an undefined variable. Either define this variable first, import it or make sure its in scope.",
                    string_table.resolve(id),
                    token_stream.tokens[token_stream.index].kind
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Make sure to import or define this variable before using it.",
                }
            )
        }

        // Other kinds of syntax errors
        _ => {
            return_syntax_error!(
                format!(
                    "Invalid token: {:?} after new variable declaration. Expect a type or assignment operator.",
                    token_stream.tokens[token_stream.index].kind
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Use a type declaration (Int, String, etc.) or assignment operator '='",
                }
            )
        }
    };

    // Check for the assignment operator next
    // If this is parameters or a struct, then we can instead break out with a comma or struct close bracket
    token_stream.advance();

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        // If end of statement, then it's unassigned.
        // For the time being, this is a syntax error.
        // When the compiler becomes more sophisticated,
        // it will be possible to statically ensure there is an assignment on all future branches.

        // Struct bracket should only be hit here in the context of the end of some parameters
        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => {
            let var_name = string_table.resolve(id);
            return_rule_error!(
                format!("Variable '{}' must be initialized with a value", var_name),
                token_stream.current_location().to_error_location(string_table), {
                    // VariableName => var_name,
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Add '= value' after the variable declaration",
                }
            )
        }

        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected Token: {:?}. Are you trying to reference a variable that doesn't exist yet?",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table), {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Check that all referenced variables are declared before use",
                }
            )
        }
    }

    // The current token should be whatever is after the assignment operator

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away
    let parsed_expr = match token_stream.current_token_kind() {
        // Struct Definition
        // TokenKind::TypeParameterBracket => {
        //     // TODO
        // }
        TokenKind::OpenParenthesis => {
            token_stream.advance();
            create_expression(
                token_stream,
                context,
                &mut data_type,
                &ownership,
                true,
                string_table,
            )?
        }

        _ => create_expression(
            token_stream,
            context,
            &mut data_type,
            &ownership,
            false,
            string_table,
        )?,
    };

    ast_log!("Created new variable of type: {}", data_type);

    Ok(Var {
        id: id,
        value: parsed_expr,
    })
}
