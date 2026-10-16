use crate::backend::cpp::generator::CodeGenerator;
use crate::backend::cpp::stmt::types::*;
use crate::frontend::parser::ast::*;

impl CodeGenerator {
    pub(crate) fn visit_machine_decl(&mut self, decl: &Decl) {
        if let Decl::MachineDecl {
            name,
            return_type,
            labels,
            handle_block,
            ..
        } = decl {
                let call_ret = if matches!(return_type, BaseType::Unknown) {
                    // fallback to finding it in call or leave
                    handle_block
                        .iter()
                        .find_map(|h| {
                            if let Decl::FnDecl {
                                name: fn_name,
                                return_type: rt,
                                ..
                            } = h
                            {
                                if fn_name == "call" || fn_name == "leave" {
                                    Some(type_to_cpp(rt))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| "void".to_string())
                } else {
                    type_to_cpp(return_type)
                };

                let call_params: Option<&Vec<Param>> = handle_block.iter().find_map(|h| {
                    if let Decl::FnDecl {
                        name: fn_name,
                        params,
                        ..
                    } = h
                    {
                        if fn_name == "call" {
                            Some(params)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });

                let mut param_strs = Vec::new();
                let mut param_arg_names = Vec::new();
                if let Some(params) = call_params {
                    for p in params {
                        param_strs.push(format!("{} {}", type_to_cpp(&p.type_node), p.name));
                        param_arg_names.push(p.name.clone());
                    }
                }

                let old_in_class = self.in_class_or_scope;
                self.in_class_or_scope = true;
                self.emit(&format!("struct {} {{", name));
                self.indent_level += 1;

                // Machine internal state
                self.emit("bool has_yielded = false;");
                self.emit("int32_t __state = 0;");

                // Hoist `this.var = value` or `this.var := value` or label variables as struct fields
                let mut field_names: Vec<String> = Vec::new();
                let mut fields_out: Vec<String> = Vec::new();

                fn collect_machine_fields(
                    s: &Stmt,
                    field_names: &mut Vec<String>,
                    fields_out: &mut Vec<String>,
                ) {
                    match s {
                        Stmt::Declaration(Decl::VarDecl {
                            name: f_name,
                            type_node,
                            ..
                        }) => {
                            if !field_names.contains(f_name) {
                                let cpp_t = type_to_cpp(type_node);
                                fields_out.push(format!("{} {} = {{}};", cpp_t, f_name));
                                field_names.push(f_name.clone());
                            }
                        }
                        Stmt::Block(inner) | Stmt::ThisBlock(inner) => {
                            for sub in inner {
                                collect_machine_fields(sub, field_names, fields_out);
                            }
                        }
                        Stmt::IfStmt {
                            then_block,
                            else_block,
                            ..
                        } => {
                            for sub in then_block {
                                collect_machine_fields(sub, field_names, fields_out);
                            }
                            if let Some(eb) = else_block {
                                for sub in eb {
                                    collect_machine_fields(sub, field_names, fields_out);
                                }
                            }
                        }
                        Stmt::WhileStmt { body, .. } | Stmt::DoWhileStmt { body, .. } => {
                            if let EitherBlock::Inline(sub) = body {
                                for st in sub {
                                    collect_machine_fields(st, field_names, fields_out);
                                }
                            }
                        }
                        Stmt::ForStmt { init, body, .. } => {
                            if let Some(i) = init {
                                collect_machine_fields(i, field_names, fields_out);
                            }
                            if let EitherBlock::Inline(sub) = body {
                                for st in sub {
                                    collect_machine_fields(st, field_names, fields_out);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // Collect from handle_block first so typed `this { ... }` declarations take precedence
                for h in handle_block {
                    if let Decl::FnDecl { body, .. } = h {
                        for s in body {
                            collect_machine_fields(s, &mut field_names, &mut fields_out);
                        }
                    }
                }
                fn collect_label_this_fields(
                    s: &Stmt,
                    field_names: &mut Vec<String>,
                    fields_out: &mut Vec<String>,
                ) {
                    match s {
                        Stmt::ThisBlock(inner) => {
                            for sub in inner {
                                if let Stmt::Declaration(Decl::VarDecl { name, type_node, .. }) = sub {
                                    if !field_names.contains(name) {
                                        let cpp_t = type_to_cpp(type_node);
                                        fields_out.push(format!("{} {} = {{}};", cpp_t, name));
                                        field_names.push(name.clone());
                                    }
                                }
                            }
                        }
                        Stmt::ReassignStmt { target, .. } => {
                            if let Expr::PropertyAccess { object, property } = target {
                                if matches!(&**object, Expr::This) && !field_names.contains(property) {
                                    fields_out.push(format!("int32_t {} = {{}};", property));
                                    field_names.push(property.clone());
                                }
                            }
                        }
                        Stmt::Block(inner) => {
                            for sub in inner {
                                collect_label_this_fields(sub, field_names, fields_out);
                            }
                        }
                        Stmt::IfStmt { then_block, else_block, .. } => {
                            for sub in then_block {
                                collect_label_this_fields(sub, field_names, fields_out);
                            }
                            if let Some(eb) = else_block {
                                for sub in eb {
                                    collect_label_this_fields(sub, field_names, fields_out);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                for label_decl in labels.iter() {
                    if let Decl::LabelDecl { body, name: lbl_name, max_runs } = label_decl {
                        let clean = lbl_name.replace("@", "");
                        if max_runs.is_some() {
                            fields_out.push(format!("size_t __run_count_{} = 0;", clean));
                        }
                        for s in body {
                            collect_machine_fields(s, &mut field_names, &mut fields_out);
                            collect_label_this_fields(s, &mut field_names, &mut fields_out);
                        }
                    }
                }

                for f in fields_out {
                    self.emit(&f);
                }

                // Call handle body logic
                let call_body: Option<&Vec<Stmt>> = handle_block.iter().find_map(|h| {
                    if let Decl::FnDecl {
                        name: fn_name,
                        body,
                        ..
                    } = h
                    {
                        if fn_name == "call" {
                            Some(body)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });

                let error_handle = handle_block.iter().find(|h| {
                    if let Decl::FnDecl { name: fn_name, .. } = h {
                        fn_name == "error"
                    } else {
                        false
                    }
                });

                // operator() with bounded labels and source-order continue
                self.emit(&format!("inline {} operator()() {{", call_ret));
                self.indent_level += 1;

                if let Some(Decl::FnDecl { .. }) = error_handle {
                    self.emit("try {");
                    self.indent_level += 1;
                }

                if let Some(body) = call_body {
                    for s in body {
                        if !matches!(s, Stmt::ThisBlock(_)) {
                            self.visit_statement(s);
                        }
                    }
                }

                // Labels in exact source order
                for i in 0..labels.len() {
                    let lbl_decl = &labels[i];
                    if let Decl::LabelDecl {
                        name: lbl_name,
                        max_runs,
                        body,
                    } = lbl_decl
                    {
                        let clean = lbl_name.replace("@", "");
                        self.emit(&format!("{}: {{", clean));
                        self.indent_level += 1;
                        if let Some(ref expr) = max_runs {
                            let limit_code = self.visit_expression(expr);
                            self.emit(&format!(
                                "if (++this->__run_count_{} > {}) {{ throw fast_std::Error((std::string(\"Runtime Error: machine '{}' label '{}' exceeded execution limit of \") + std::to_string({})).c_str()); }}",
                                clean, limit_code, name, lbl_name, limit_code
                            ));
                        }
                        for s in body {
                            match s {
                                Stmt::Declaration(Decl::VarDecl {
                                    name: f_name,
                                    value,
                                    assign_op: _,
                                    ..
                                }) => {
                                    if field_names.contains(f_name) {
                                        let val_code = self.visit_expression(value);
                                        self.emit(&format!("this->{} = {};", f_name, val_code));
                                    } else {
                                        self.visit_statement(s);
                                    }
                                }
                                Stmt::ContinueStmt => {
                                    if i + 1 < labels.len() {
                                        if let Decl::LabelDecl {
                                            name: next_name, ..
                                        } = &labels[i + 1]
                                        {
                                            let next_clean = next_name.replace("@", "");
                                            self.emit(&format!("goto {};", next_clean));
                                        }
                                    } else {
                                        self.emit("return leave();");
                                    }
                                }
                                Stmt::BreakStmt => {
                                    self.emit("return leave();");
                                }
                                Stmt::LeaveStmt => {
                                    self.emit("return leave();");
                                }
                                Stmt::YieldStmt(opt_expr) => {
                                    self.emit("this->has_yielded = true;");
                                    if let Some(e) = opt_expr {
                                        let val = self.visit_expression(e);
                                        self.emit(&format!("return {};", val));
                                    } else {
                                        self.emit("return this->yield();");
                                    }
                                }
                                _ => self.visit_statement(s),
                            }
                        }

                        // implicit leave at end of bounded label block to prevent fallthrough
                        self.emit("return leave();");

                        self.indent_level -= 1;
                        self.emit("}");
                    }
                }

                self.emit("return leave();");

                if let Some(Decl::FnDecl {
                    params: err_params,
                    body: err_body,
                    ..
                }) = error_handle
                {
                    self.indent_level -= 1;
                    let catch_param = if !err_params.is_empty() {
                        err_params[0].name.clone()
                    } else {
                        "__e".to_string()
                    };
                    self.emit(&format!(
                        "}} catch (const fast_std::Error& {}) {{",
                        catch_param
                    ));
                    self.indent_level += 1;
                    for s in err_body {
                        self.visit_statement(s);
                    }
                    if call_ret != "void" {
                        self.emit("return leave();");
                    }
                    self.indent_level -= 1;
                    self.emit(&format!(
                        "}} catch (const fast_std::Error* __{}_ptr) {{",
                        catch_param
                    ));
                    self.indent_level += 1;
                    self.emit(&format!(
                        "const fast_std::Error& {} = *__{}_ptr;",
                        catch_param, catch_param
                    ));
                    for s in err_body {
                        self.visit_statement(s);
                    }
                    if call_ret != "void" {
                        self.emit("return leave();");
                    }
                    self.indent_level -= 1;
                    self.emit("}");
                }

                self.indent_level -= 1;
                self.emit("}");

                // Overloaded operator() with arguments if params exist
                if !param_strs.is_empty() {
                    let mut assigns = Vec::new();
                    if let Some(params) = call_params {
                        for p in params {
                            assigns.push(format!("this->{} = {};", p.name, p.name));
                        }
                    }
                    self.emit(&format!(
                        "inline {} operator()({}) {{ {} return (*this)(); }}",
                        call_ret,
                        param_strs.join(", "),
                        assigns.join(" ")
                    ));
                }

                // Emit `call()` methods
                if !param_strs.is_empty() {
                    let mut assigns = Vec::new();
                    if let Some(params) = call_params {
                        for p in params {
                            assigns.push(format!("this->{} = {};", p.name, p.name));
                        }
                    }
                    self.emit(&format!(
                        "inline {} call({}) {{ {} return (*this)(); }}",
                        call_ret,
                        param_strs.join(", "),
                        assigns.join(" ")
                    ));
                }
                self.emit(&format!(
                    "inline {} call() {{ return (*this)(); }}",
                    call_ret
                ));

                // Emit `leave` handle as a separate method
                let mut found_leave = false;
                for h in handle_block.iter() {
                    if let Decl::FnDecl {
                        name: fn_name,
                        return_type,
                        body,
                        ..
                    } = h
                    {
                        if fn_name == "leave" {
                            found_leave = true;
                            let ret_str = type_to_cpp(return_type);
                            self.emit(&format!("{} leave() {{", ret_str));
                            self.indent_level += 1;
                            for s in body {
                                self.visit_statement(s);
                            }
                            if ret_str != "void" {
                                self.emit("return {};");
                            }
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                    }
                }
                if !found_leave {
                    self.emit(&format!(
                        "{} leave() {{ {} }}",
                        call_ret,
                        if call_ret == "void" { "" } else { "return {};" }
                    ));
                }

                // Emit `yield` handle as a separate method
                let mut found_yield = false;
                for h in handle_block.iter() {
                    if let Decl::FnDecl {
                        name: fn_name,
                        return_type,
                        body,
                        ..
                    } = h
                    {
                        if fn_name == "yield" {
                            found_yield = true;
                            let ret_str = type_to_cpp(return_type);
                            self.emit(&format!("{} yield() {{", ret_str));
                            self.indent_level += 1;
                            for s in body {
                                self.visit_statement(s);
                            }
                            if ret_str != "void" {
                                self.emit("return {};");
                            }
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                    }
                }
                if !found_yield {
                    self.emit(&format!(
                        "{} yield() {{ {} }}",
                        call_ret,
                        if call_ret == "void" { "" } else { "return {};" }
                    ));
                }

                // Default constructor
                self.emit(&format!("{}() {{}}", name));

                // Optional constructor with call_params
                if let Some(params) = call_params {
                    if !params.is_empty() {
                        let mut ctor_init = Vec::new();
                        for p in params {
                            ctor_init.push(format!("this->{} = {};", p.name, p.name));
                        }
                        self.emit(&format!(
                            "{}({}) {{ {} }}",
                            name,
                            param_strs.join(", "),
                            ctor_init.join(" ")
                        ));
                    }
                }

                self.indent_level -= 1;
                self.emit("};");
                self.in_class_or_scope = old_in_class;
            }


    }
}
