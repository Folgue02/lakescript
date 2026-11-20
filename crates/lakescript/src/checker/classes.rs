//! Checking classes *(see `ideas/language/classes.md`)*: their signatures are
//! made when their block starts, like functions', and their code is checked
//! where they're declared.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::types::{ClassBody, ClassRef, ClassSig, FieldSig, FnTy, MemberSig, Ty, a};
use super::{Check, Checker, Return, This, block_terminates};
use crate::ast::*;
use crate::token::Span;

impl Checker<'_> {
    /// Gives a class its name in the block, before any class of the block has a body.
    pub(super) fn declare_class(&mut self, decl: &ClassDecl, span: Span) -> Check<ClassRef> {
        let scope = self.scopes.last().unwrap();
        if scope.types.contains_key(&decl.name) || scope.funcs.contains_key(&decl.name) {
            return self.fail(format!("`{}` already exists", decl.name), span);
        }
        let interface = decl.kind == ClassKind::Interface;
        let is_enum = decl.kind == ClassKind::Enum;
        let class = ClassRef(Rc::new(ClassSig { name: Rc::from(decl.name.as_str()), interface, is_enum, body: RefCell::new(ClassBody::default()) }));
        let depth = self.returns.len();
        let scope = self.scopes.last_mut().unwrap();
        scope.types.insert(decl.name.clone(), Ty::Object(class.clone()));
        scope.pending.insert(decl.name.clone());
        scope.depth = depth;
        Ok(class)
    }

    /// The types of a class's fields and methods, and what calling it takes.
    pub(super) fn class_body(&mut self, decl: &ClassDecl, class: &ClassRef, span: Span) -> Check<()> {
        // In an interface's signatures, `Self` is the class that implements it
        let self_param = class.0.interface;
        self.this.push(This { class: class.clone(), self_param, is_static: false, constructor: false });
        let result = self.class_body_inner(decl, class, span);
        self.this.pop();
        result
    }

    fn class_body_inner(&mut self, decl: &ClassDecl, class: &ClassRef, span: Span) -> Check<()> {
        if !decl.generics.is_empty() {
            return self.fail("generic classes aren't ready yet", span);
        }
        let mut body = ClassBody::default();
        let interface = class.0.interface;
        // The interfaces it implements, or the ones an interface extends
        let implemented: Vec<&TypeExpr> = if interface { decl.extends.iter().collect() } else { decl.implements.iter().collect() };
        for ty in implemented {
            match self.ty(ty)? {
                Ty::Object(i) if i.0.interface => {
                    if i.is_a(class) {
                        return self.fail(format!("`{}` can't extend itself, directly or through other interfaces", decl.name), ty.span);
                    }
                    body.interfaces.push(i);
                }
                other => {
                    let what = if interface { "extend" } else { "implement" };
                    return self.fail(format!("a {} can only {what} interfaces, and {} isn't one", decl.kind.word(), a(&other)), ty.span);
                }
            }
        }
        if let (false, Some(parent)) = (interface, decl.extends.first()) {
            let t = self.ty(parent)?;
            match t {
                Ty::Object(p) if p.0.interface => {
                    return self.fail(
                        format!("`{}` is an interface: a class implements it with an `implement {} {{ ... }}` block, it doesn't extend it", p.0.name, p.0.name),
                        parent.span,
                    );
                }
                Ty::Object(p) => {
                    if p.is_a(class) {
                        return self.fail(format!("`{}` can't extend itself, directly or through other classes", decl.name), parent.span);
                    }
                    body.parent = Some(p);
                }
                other => return self.fail(format!("a class can only extend another class, and {} isn't one", a(&other)), parent.span),
            }
        }
        let inherited: Vec<FieldSig> = body.parent.as_ref().map(all_fields).unwrap_or_default();
        for member in &decl.members {
            match &member.kind {
                MemberKind::Field { constant, ty, name, value } => {
                    let t = self.ty(ty)?;
                    let field = FieldSig { name: name.clone(), ty: t, public: member.public, constant: *constant, has_value: value.is_some() };
                    if member.is_static {
                        if body.statics.contains_key(name) {
                            return self.fail(format!("`{}` has two static fields named `${name}`", decl.name), member.span);
                        }
                        body.statics.insert(name.clone(), field);
                    } else {
                        if body.fields.iter().chain(inherited.iter()).any(|f| f.name == *name) {
                            return self.fail(format!("`{}` already has a field named `{name}`", decl.name), member.span);
                        }
                        body.fields.push(field);
                    }
                }
                MemberKind::Method { decl: method, body: has_body } => {
                    let sig = self.method_signature(method, member.span)?;
                    // The methods of interfaces, and of the blocks that implement them, are always public
                    let public = member.public || interface || matches!(member.origin, Origin::Implement(_));
                    let entry = MemberSig { ty: Rc::new(sig), public, has_body: *has_body };
                    let table = if member.is_static { &mut body.static_methods } else { &mut body.methods };
                    if table.contains_key(&method.name) {
                        return self.fail(format!("`{}` has two methods named `{}`", decl.name, method.name), member.span);
                    }
                    table.insert(method.name.clone(), entry);
                }
                MemberKind::Constructor(_) => {}
            }
        }
        if interface {
            *class.0.body.borrow_mut() = body;
            return Ok(());
        }
        // What calling the class takes
        let ctor = decl.members.iter().find_map(|m| match &m.kind {
            MemberKind::Constructor(d) => Some((d, m.span)),
            _ => None,
        });
        let every_field: Vec<FieldSig> = inherited.iter().chain(body.fields.iter()).cloned().collect();
        body.constructor = Some(Rc::new(match ctor {
            Some((d, ctor_span)) => {
                body.own_constructor = true;
                let mut sig = self.method_signature(d, ctor_span)?;
                for (i, p) in d.params.iter().enumerate() {
                    if p.field {
                        match every_field.iter().find(|f| f.name == p.name) {
                            Some(f) => sig.params[i] = f.ty.clone(),
                            None => return self.fail(format!("`{}` has no field named `{}`", decl.name, p.name), ctor_span),
                        }
                    }
                }
                sig.ret = Ty::Object(class.clone());
                sig
            }
            None => {
                // Every field, the superclass's first
                let required = every_field.iter().take_while(|f| !f.has_value && !matches!(f.ty, Ty::Nullable(_) | Ty::Any)).count();
                FnTy {
                    params: every_field.iter().map(|f| f.ty.clone()).collect(),
                    names: every_field.iter().map(|f| f.name.clone()).collect(),
                    required,
                    variadic: false,
                    ret: Ty::Object(class.clone()),
                    generics: Vec::new(),
                }
            }
        }));
        let ctor_sig = body.constructor.clone().unwrap();
        if class.0.is_enum {
            self.enum_members(decl, class, &mut body)?;
            *class.0.body.borrow_mut() = body;
            return Ok(());
        }
        *class.0.body.borrow_mut() = body;
        // Calling the class is calling its constructor
        let scope = self.scopes.last_mut().unwrap();
        scope.funcs.insert(decl.name.clone(), ctor_sig);
        Ok(())
    }

    /// The values of an enum, and the methods every enum has.
    fn enum_members(&mut self, decl: &ClassDecl, class: &ClassRef, body: &mut ClassBody) -> Check<()> {
        let this = Ty::Object(class.clone());
        for value in &decl.values {
            if body.values.contains(&value.name) {
                return self.fail(format!("`{}` has two values named `{}`", decl.name, value.name), value.span);
            }
            body.values.push(value.name.clone());
        }
        let method = |params: Vec<Ty>, names: Vec<&str>, ret: Ty| MemberSig {
            ty: Rc::new(FnTy { required: params.len(), params, names: names.into_iter().map(String::from).collect(), variadic: false, ret, generics: Vec::new() }),
            public: true,
            has_body: true,
        };
        let builtins = [
            ("values", method(Vec::new(), Vec::new(), Ty::List(Box::new(this.clone())))),
            ("from", method(vec![Ty::Str], vec!["name"], this.clone())),
            ("fromIgnoreCase", method(vec![Ty::Str], vec!["name"], this.clone())),
        ];
        for (name, sig) in builtins {
            if body.static_methods.contains_key(name) || body.values.iter().any(|v| v == name) {
                return self.fail(format!("every enum has `{name}()`, so `{}` can't declare it", decl.name), decl.span);
            }
            body.static_methods.insert(name.to_string(), sig);
        }
        if body.methods.contains_key("name") {
            return self.fail(format!("every enum value has `name()`, so `{}` can't declare it", decl.name), decl.span);
        }
        body.methods.insert("name".into(), method(Vec::new(), Vec::new(), Ty::Str));
        Ok(())
    }

    fn method_signature(&mut self, method: &FunctionDecl, span: Span) -> Check<FnTy> {
        self.push();
        self.scopes.last_mut().unwrap().generics.extend(method.generics.iter().cloned());
        let sig = self.signature(method, span);
        self.pop();
        sig
    }

    /// The code of a class, where it's declared.
    pub(super) fn class(&mut self, decl: &ClassDecl, span: Span) -> Check<()> {
        let Some(Ty::Object(class)) = self.scopes.last().unwrap().types.get(&decl.name).cloned() else {
            return self.fail(format!("classes can only be declared at the start of a block's statements, and `{}` isn't", decl.name), span);
        };
        self.scopes.last_mut().unwrap().pending.remove(&decl.name);
        if !class.0.interface {
            self.check_implements(decl, &class)?;
        }
        if class.0.is_enum {
            // Each value calls the enum's constructor
            let ctor = class.0.body.borrow().constructor.clone().unwrap();
            for value in &decl.values {
                self.this.push(This { class: class.clone(), self_param: false, is_static: true, constructor: false });
                let result = self.call(&ctor, &HashMap::new(), &value.args, &value.name, value.span);
                self.this.pop();
                result?;
            }
        }
        let parent = class.0.body.borrow().parent.clone();
        // A constructor of its own is needed when the superclass has one
        let own_ctor = class.0.body.borrow().own_constructor;
        if let Some(p) = &parent {
            if !own_ctor {
                if let Some((_, owner)) = p.find(|b| b.own_constructor.then_some(())) {
                    return self.fail(
                        format!("`{}` needs its own `new`, which calls `@super(...)`, since its superclass `{}` has one", decl.name, owner.0.name),
                        span,
                    );
                }
            }
        }
        for member in &decl.members {
            match &member.kind {
                MemberKind::Field { name, value: Some(value), .. } => {
                    let ty = if member.is_static {
                        class.0.body.borrow().statics[name].ty.clone()
                    } else {
                        class.0.body.borrow().fields.iter().find(|f| f.name == *name).unwrap().ty.clone()
                    };
                    self.this.push(This { class: class.clone(), self_param: false, is_static: member.is_static, constructor: false });
                    let found = self.expr(value, Some(&ty));
                    self.this.pop();
                    let found = found?;
                    if !self.assignable(&found, &ty) {
                        let found = self.resolve(&found);
                        return self.fail(format!("`{name}` is {}, so it can't hold {}", a(&ty), a(&found)), value.span);
                    }
                }
                MemberKind::Field { .. } => {}
                MemberKind::Method { decl: method, body } => {
                    if !body {
                        continue;
                    }
                    let table = if member.is_static { "static_methods" } else { "methods" };
                    let mut sig = {
                        let b = class.0.body.borrow();
                        let t = if table == "methods" { &b.methods } else { &b.static_methods };
                        t[&method.name].ty.clone()
                    };
                    if class.0.interface {
                        // A default is checked with `Self` as the interface: it works for any class that implements it
                        let map = HashMap::from([(Rc::from("Self"), Ty::Object(class.clone()))]);
                        if let Ty::Func(f) = Ty::Func(sig.clone()).substitute(&map) {
                            sig = f;
                        }
                    }
                    self.check_override(decl, &class, member, method)?;
                    self.this.push(This { class: class.clone(), self_param: false, is_static: member.is_static, constructor: false });
                    let result = self.method_body(method, &sig, member.span);
                    self.this.pop();
                    result?;
                }
                MemberKind::Constructor(ctor) => {
                    let sig = class.0.body.borrow().constructor.clone().unwrap();
                    if parent.is_some() && !calls_super(&ctor.body) {
                        return self.fail(format!("the constructor of `{}` must call `@super(...)`, the constructor of its superclass", decl.name), member.span);
                    }
                    self.this.push(This { class: class.clone(), self_param: false, is_static: false, constructor: true });
                    let result = self.method_body(ctor, &sig, member.span);
                    self.this.pop();
                    result?;
                }
            }
        }
        Ok(())
    }

    /// A method in an `override` block must replace one of the superclass,
    /// and one outside it must not.
    fn check_override(&mut self, decl: &ClassDecl, class: &ClassRef, member: &Member, method: &FunctionDecl) -> Check<()> {
        if member.is_static {
            if member.origin == Origin::Override {
                return self.fail("static methods can't be overridden", member.span);
            }
            return Ok(());
        }
        if class.0.interface {
            return Ok(());
        }
        let parent = class.0.body.borrow().parent.clone();
        let inherited = parent.as_ref().and_then(|p| p.find(|b| b.methods.get(&method.name).cloned()));
        if member.origin == Origin::Own {
            if let Some(i) = class.all_interfaces().into_iter().find(|i| i.find_method(&method.name).is_some()) {
                return self.fail(
                    format!("`{}` is a method of `{}`, so it goes in its `implement {} {{ ... }}` block", method.name, i.0.name, i.0.name),
                    member.span,
                );
            }
        }
        match (&member.origin, inherited) {
            (Origin::Override, None) => {
                self.fail(format!("`{}` is in `override`, but the superclass of `{}` has no method with that name", method.name, decl.name), member.span)
            }
            (Origin::Own, Some((_, owner))) => self.fail(
                format!("`{}` replaces the method of `{}`, so it goes in an `override {{ ... }}` block", method.name, owner.0.name),
                member.span,
            ),
            (Origin::Override, Some((old, owner))) => {
                let new = class.0.body.borrow().methods[&method.name].ty.clone();
                let same = old.ty.params.len() == new.params.len()
                    && old.ty.params.iter().zip(new.params.iter()).all(|(o, n)| o == n)
                    && self.assignable(&new.ret, &old.ty.ret);
                if !same {
                    return self.fail(
                        format!("`{}` must take the same parameters as the method it overrides in `{}`, and return what it returns", method.name, owner.0.name),
                        member.span,
                    );
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Each `implement` block has methods of its interface, with the same
    /// signatures, and the class has every method of its interfaces without a default.
    fn check_implements(&mut self, decl: &ClassDecl, class: &ClassRef) -> Check<()> {
        let this = Ty::Object(class.clone());
        let map = HashMap::from([(Rc::from("Self"), this)]);
        for member in &decl.members {
            let (Origin::Implement(ty), MemberKind::Method { decl: method, .. }) = (&member.origin, &member.kind) else {
                continue;
            };
            let Ty::Object(interface) = self.ty(ty)? else { continue };
            if member.is_static {
                return self.fail("the methods of an `implement` block can't be `static`", member.span);
            }
            let Some((expected, owner)) = interface.find_method(&method.name) else {
                return self.fail(format!("`{}` has no method `{}`, so it can't go in its `implement` block", interface.0.name, method.name), member.span);
            };
            let Ty::Func(expected) = Ty::Func(expected.ty.clone()).substitute(&map) else { unreachable!() };
            let found = class.0.body.borrow().methods[&method.name].ty.clone();
            let same = expected.params.len() == found.params.len()
                && expected.params.iter().zip(found.params.iter()).all(|(e, f)| e == f)
                && self.assignable(&found.ret, &expected.ret);
            if !same {
                let params: Vec<String> = expected.params.iter().map(|p| p.to_string()).collect();
                return self.fail(
                    format!("`{}` must be `{} {}({})`, like in `{}`", method.name, expected.ret, method.name, params.join(", "), owner.0.name),
                    member.span,
                );
            }
        }
        // Every method without a default, of every interface it implements
        let own: Vec<ClassRef> = class.0.body.borrow().interfaces.clone();
        for interface in own {
            let mut all = vec![interface.clone()];
            all.extend(interface.all_interfaces());
            for i in all {
                let required: Vec<String> = i.0.body.borrow().methods.iter().filter(|(_, m)| !m.has_body).map(|(n, _)| n.clone()).collect();
                for name in required {
                    let implemented = class.find(|b| b.methods.get(&name).filter(|m| m.has_body).cloned()).is_some();
                    if !implemented {
                        return self.fail(format!("`{}` implements `{}`, but it has no method `{name}`", decl.name, i.0.name), decl.span);
                    }
                }
            }
        }
        Ok(())
    }

    fn method_body(&mut self, method: &FunctionDecl, sig: &FnTy, span: Span) -> Check<()> {
        let ret = sig.ret.clone();
        self.push();
        self.scopes.last_mut().unwrap().generics.extend(method.generics.iter().cloned());
        // The parameters written as fields aren't variables
        let mut plain = method.clone();
        let mut plain_sig = FnTy {
            params: sig.params.clone(),
            names: sig.names.clone(),
            required: sig.required,
            variadic: sig.variadic,
            ret: sig.ret.clone(),
            generics: sig.generics.clone(),
        };
        let fields: Vec<usize> = plain.params.iter().enumerate().filter(|(_, p)| p.field).map(|(i, _)| i).collect();
        for i in fields.into_iter().rev() {
            let p = plain.params.remove(i);
            let t = plain_sig.params.remove(i);
            if let Some(default) = &p.default {
                let found = self.expr(default, Some(&t))?;
                if !self.assignable(&found, &t) {
                    let found = self.resolve(&found);
                    return self.fail(format!("the default of `@{}` is {}, but the field is {}", p.name, a(&found), a(&t)), default.span);
                }
            }
        }
        let is_ctor = method.name == "new";
        let declared = if is_ctor { Ty::Void } else { ret.clone() };
        let result = self.body(&plain, &plain_sig, Return::Declared(declared), span);
        self.pop();
        result?;
        if !is_ctor && !matches!(ret, Ty::Void | Ty::Any) {
            if let FunctionBody::Block(stmts) = &method.body {
                if !block_terminates(stmts) {
                    return self.fail(format!("`{}` must return {} at the end", method.name, a(&ret)), span);
                }
            }
        }
        Ok(())
    }

    /// The class a name means here: `Self` is the class being checked.
    pub(super) fn class_named(&self, name: &str) -> Option<ClassRef> {
        if name == "Self" {
            return self.this.last().map(|t| t.class.clone());
        }
        match self.scopes.iter().rev().find_map(|s| s.types.get(name)) {
            Some(Ty::Object(c)) => Some(c.clone()),
            _ => None,
        }
    }

    /// Private members can only be used inside the class that declares them.
    pub(super) fn check_access(&mut self, public: bool, owner: &ClassRef, what: &str, span: Span) -> Check<()> {
        if public || self.this.iter().any(|t| t.class == *owner) {
            return Ok(());
        }
        self.fail(format!("{what} is private to `{}`: only its own code can use it. Mark it `pub` to share it", owner.0.name), span)
    }

    /// `$object.name`
    pub(super) fn field(&mut self, target: &Expr, name: &str, span: Span) -> Check<Ty> {
        let t = self.expr(target, None)?;
        match self.shallow(&t) {
            Ty::Object(class) => {
                let Some((field, owner)) = class.find(|b| b.fields.iter().find(|f| f.name == name).cloned()) else {
                    if class.find(|b| b.methods.get(name).cloned()).is_some() {
                        return self.fail(format!("`{name}` is a method of `{}`: call it with `.{name}()`", class.0.name), span);
                    }
                    return self.fail(format!("`{}` has no field named `{name}`", class.0.name), span);
                };
                self.check_access(field.public, &owner, &format!("the field `{name}`"), span)?;
                Ok(field.ty)
            }
            Ty::Any | Ty::Var(_) => Ok(Ty::Any),
            Ty::Nullable(_) => self.fail(format!("this can be `null`, so `.{name}` can't always be read. Check for `null` first"), target.span),
            other => self.fail(format!("{} has no fields: `.{name}` only works on objects. Methods are called with parentheses: `.{name}()`", a(&other)), span),
        }
    }

    /// `Class::$name` or `Class::name`
    pub(super) fn static_member(&mut self, class: &str, name: &str, variable: bool, span: Span) -> Check<Ty> {
        let Some(c) = self.class_named(class) else {
            if class == "Self" {
                return self.fail("`Self` can only be used inside a class", span);
            }
            return self.fail(format!("there's no class named `{class}`"), span);
        };
        if !variable && c.0.body.borrow().values.iter().any(|v| v == name) {
            return Ok(Ty::Object(c));
        }
        if variable {
            let field = c.0.body.borrow().statics.get(name).cloned();
            let Some(field) = field else {
                return self.fail(format!("`{}` has no static field named `${name}`", c.0.name), span);
            };
            self.check_access(field.public, &c, &format!("`{}::${name}`", c.0.name), span)?;
            Ok(field.ty)
        } else {
            let method = c.0.body.borrow().static_methods.get(name).cloned();
            let Some(method) = method else {
                if c.0.is_enum && !name.chars().any(|ch| ch.is_ascii_lowercase()) {
                    let values = c.0.body.borrow().values.join(", ");
                    return self.fail(format!("`{}` has no value `{name}`: it has {values}", c.0.name), span);
                }
                if c.0.body.borrow().methods.contains_key(name) {
                    return self.fail(format!("`{name}` isn't static: it's called on an object of `{}`", c.0.name), span);
                }
                return self.fail(format!("`{}` has no static method named `{name}`", c.0.name), span);
            };
            self.check_access(method.public, &c, &format!("`{}::{name}()`", c.0.name), span)?;
            Ok(Ty::Func(method.ty))
        }
    }
}

/// Every field of a class's objects, the superclasses' first.
fn all_fields(class: &ClassRef) -> Vec<FieldSig> {
    let body = class.0.body.borrow();
    let mut fields = body.parent.as_ref().map(all_fields).unwrap_or_default();
    fields.extend(body.fields.iter().cloned());
    fields
}

/// Whether a constructor's code has a `@super(...)`.
fn calls_super(body: &FunctionBody) -> bool {
    // The printed tree has `(@super ...)` for the call, and `(@super.name ...)` for a method
    let has = |text: String| text.contains("(@super)") || text.contains("(@super ");
    match body {
        FunctionBody::Expr(e) => has(e.to_string()),
        FunctionBody::Block(stmts) => stmts.iter().any(|s| has(s.to_string())),
    }
}
