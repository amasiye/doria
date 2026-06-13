use crate::{ast, ir};

pub fn lower_program(program: &ast::Program) -> ir::Program {
    ir::Program {
        items: program.items.iter().map(lower_item).collect(),
    }
}

fn lower_item(item: &ast::Item) -> ir::Item {
    match item {
        ast::Item::Class(class_decl) => ir::Item::Class(lower_class(class_decl)),
        ast::Item::Function(function) => ir::Item::Function(lower_function(function)),
        ast::Item::Statement(statement) => ir::Item::Statement(lower_stmt(statement)),
    }
}

fn lower_class(class_decl: &ast::ClassDecl) -> ir::ClassDecl {
    ir::ClassDecl {
        name: class_decl.name.clone(),
        members: class_decl.members.iter().map(lower_class_member).collect(),
        span: class_decl.span,
    }
}

fn lower_class_member(member: &ast::ClassMember) -> ir::ClassMember {
    match member {
        ast::ClassMember::Property(property) => ir::ClassMember::Property(lower_property(property)),
        ast::ClassMember::Method(method) => ir::ClassMember::Method(lower_function(method)),
    }
}

fn lower_property(property: &ast::PropertyDecl) -> ir::PropertyDecl {
    ir::PropertyDecl {
        visibility: property.visibility.clone(),
        writable: property.writable,
        ty: property.ty.clone(),
        name: property.name.clone(),
        initializer: property.initializer.as_ref().map(lower_expr),
        span: property.span,
    }
}

fn lower_function(function: &ast::FunctionDecl) -> ir::FunctionDecl {
    ir::FunctionDecl {
        visibility: function.visibility.clone(),
        writable_this: function.writable_this,
        name: function.name.clone(),
        params: function.params.iter().map(lower_param).collect(),
        return_type: function.return_type.clone(),
        body: lower_block(&function.body),
        span: function.span,
    }
}

fn lower_param(param: &ast::Param) -> ir::Param {
    ir::Param {
        promoted_visibility: param.promoted_visibility.clone(),
        writable: param.writable,
        ty: param.ty.clone(),
        name: param.name.clone(),
        default: param.default.as_ref().map(lower_expr),
        span: param.span,
    }
}

fn lower_block(block: &ast::Block) -> ir::Block {
    ir::Block {
        statements: block.statements.iter().map(lower_stmt).collect(),
        span: block.span,
    }
}

fn lower_stmt(statement: &ast::Stmt) -> ir::Stmt {
    match statement {
        ast::Stmt::VarDecl(decl) => ir::Stmt::VarDecl(ir::VarDecl {
            writable: decl.writable,
            ty: decl.ty.clone(),
            name: decl.name.clone(),
            initializer: lower_expr(&decl.initializer),
            span: decl.span,
        }),
        ast::Stmt::Assignment(assignment) => ir::Stmt::Assignment(ir::Assignment {
            target: lower_expr(&assignment.target),
            op: assignment.op.clone(),
            value: lower_expr(&assignment.value),
            span: assignment.span,
        }),
        ast::Stmt::Echo { expr, span } => ir::Stmt::Echo {
            expr: lower_expr(expr),
            span: *span,
        },
        ast::Stmt::Return { expr, span } => ir::Stmt::Return {
            expr: expr.as_ref().map(lower_expr),
            span: *span,
        },
        ast::Stmt::Foreach(foreach) => ir::Stmt::Foreach(ir::ForeachStmt {
            iterable: lower_expr(&foreach.iterable),
            key: foreach.key.as_ref().map(lower_foreach_binding),
            value: lower_foreach_binding(&foreach.value),
            body: lower_block(&foreach.body),
            span: foreach.span,
        }),
        ast::Stmt::Expr { expr, span } => ir::Stmt::Expr {
            expr: lower_expr(expr),
            span: *span,
        },
    }
}

fn lower_foreach_binding(binding: &ast::ForeachBinding) -> ir::ForeachBinding {
    ir::ForeachBinding {
        ty: binding.ty.clone(),
        name: binding.name.clone(),
    }
}

fn lower_expr(expr: &ast::Expr) -> ir::Expr {
    match expr {
        ast::Expr::Variable { name, span } => ir::Expr::Variable {
            name: name.clone(),
            span: *span,
        },
        ast::Expr::This { span } => ir::Expr::This { span: *span },
        ast::Expr::Identifier { name, span } => ir::Expr::Identifier {
            name: name.clone(),
            span: *span,
        },
        ast::Expr::String { value, span } => ir::Expr::String {
            value: value.clone(),
            span: *span,
        },
        ast::Expr::Int { value, span } => ir::Expr::Int {
            value: value.clone(),
            span: *span,
        },
        ast::Expr::Float { value, span } => ir::Expr::Float {
            value: value.clone(),
            span: *span,
        },
        ast::Expr::Bool { value, span } => ir::Expr::Bool {
            value: *value,
            span: *span,
        },
        ast::Expr::Null { span } => ir::Expr::Null { span: *span },
        ast::Expr::Array { elements, span } => ir::Expr::Array {
            elements: elements.iter().map(lower_array_element).collect(),
            span: *span,
        },
        ast::Expr::PropertyAccess {
            object,
            property,
            span,
        } => ir::Expr::PropertyAccess {
            object: Box::new(lower_expr(object)),
            property: property.clone(),
            span: *span,
        },
        ast::Expr::MethodCall {
            object,
            method,
            args,
            span,
        } => ir::Expr::MethodCall {
            object: Box::new(lower_expr(object)),
            method: method.clone(),
            args: args.iter().map(lower_expr).collect(),
            span: *span,
        },
        ast::Expr::FunctionCall { name, args, span } => ir::Expr::FunctionCall {
            name: name.clone(),
            args: args.iter().map(lower_expr).collect(),
            span: *span,
        },
        ast::Expr::StaticCall {
            class_name,
            method,
            args,
            span,
        } => ir::Expr::StaticCall {
            class_name: class_name.clone(),
            method: method.clone(),
            args: args.iter().map(lower_expr).collect(),
            span: *span,
        },
        ast::Expr::New {
            class_name,
            args,
            span,
        } => ir::Expr::New {
            class_name: class_name.clone(),
            args: args.iter().map(lower_expr).collect(),
            span: *span,
        },
        ast::Expr::Binary {
            left,
            op,
            right,
            span,
        } => ir::Expr::Binary {
            left: Box::new(lower_expr(left)),
            op: op.clone(),
            right: Box::new(lower_expr(right)),
            span: *span,
        },
    }
}

fn lower_array_element(element: &ast::ArrayElement) -> ir::ArrayElement {
    ir::ArrayElement {
        key: element.key.as_ref().map(lower_expr),
        value: lower_expr(&element.value),
    }
}
