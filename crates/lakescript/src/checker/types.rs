//! Types as the checker sees them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    /// Not checked: any value, including `null`.
    Any,
    Void,
    /// What a function that never returns gives *(i.e. `exit()`)*: no value
    /// at all, so it fits wherever any type is expected, like `throw`.
    Never,
    /// The type of the `null` literal. Only nullable types accept it.
    Null,
    Bool,
    Number,
    Str,
    Regex,
    Bytes,
    /// A type given as a value: `type<User>` for `User` or `list<User>` written as an argument.
    TypeOf(Box<Ty>),
    Range,
    Command,
    /// One of the built-in exceptions *(i.e. `"ValueError"`)*.
    Exception(Rc<str>),
    Result(Box<Ty>, Box<Ty>),
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Func(Rc<FnTy>),
    Nullable(Box<Ty>),
    /// A type that isn't known yet, found out from how the value is used
    /// *(i.e. the elements of `[]`)*.
    Var(usize),
    /// A type parameter of a generic function, inside its body *(i.e. `T`)*.
    Param(Rc<str>),
    /// An object of a class, with the types of its type parameters *(i.e. `Stack<string>`)*.
    Object(ClassRef, Vec<Ty>),
}

/// A class as the checker sees it. Two are the same class only if they're the
/// same declaration, so they're compared by pointer.
#[derive(Clone)]
pub struct ClassRef(pub Rc<ClassSig>);

impl PartialEq for ClassRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for ClassRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.name)
    }
}

impl ClassRef {
    /// Its type inside its own code: its type parameters stay as they are.
    pub fn this_type(&self) -> Ty {
        Ty::Object(self.clone(), self.0.generics.iter().map(|g| Ty::Param(g.clone())).collect())
    }

    /// What each type parameter is, for these types.
    pub fn map(&self, args: &[Ty]) -> HashMap<Rc<str>, Ty> {
        self.0.generics.iter().enumerate().map(|(i, g)| (g.clone(), args.get(i).cloned().unwrap_or(Ty::Any))).collect()
    }

    /// The types of `target`'s type parameters, for an object of this class
    /// with these types, for every way it is a `target` *(i.e. `[number]` and
    /// `[Vector]` for a class that implements `IMul<number>` and `IMul<Vector>`)*.
    pub fn ancestor_args(&self, args: &[Ty], target: &ClassRef) -> Vec<Vec<Ty>> {
        if self == target {
            return vec![args.to_vec()];
        }
        let map = self.map(args);
        let body = self.0.body.borrow();
        let mut out = Vec::new();
        if let Some(p) = &body.parent {
            let pa: Vec<Ty> = body.parent_args.iter().map(|t| t.substitute(&map)).collect();
            out.extend(p.ancestor_args(&pa, target));
        }
        for (i, iface) in body.interfaces.iter().enumerate() {
            let ia: Vec<Ty> = body.interface_args.get(i).map(|a| a.iter().map(|t| t.substitute(&map)).collect()).unwrap_or_default();
            out.extend(iface.ancestor_args(&ia, target));
        }
        out
    }

    /// The type parameters of `owner` (a class this one is) mapped to types,
    /// for an object of this class with these types.
    pub fn member_map(&self, args: &[Ty], owner: &ClassRef) -> HashMap<Rc<str>, Ty> {
        let found = self.ancestor_args(args, owner).into_iter().next().unwrap_or_default();
        owner.map(&found)
    }

    /// `true` if it's this class, one of its superclasses, or an interface
    /// they implement (also through the interfaces those extend).
    pub fn is_a(&self, other: &ClassRef) -> bool {
        if self == other {
            return true;
        }
        let body = self.0.body.borrow();
        body.parent.as_ref().is_some_and(|p| p.is_a(other)) || body.interfaces.iter().any(|i| i.is_a(other))
    }

    /// A method of the class, its superclasses or its interfaces, with the
    /// class or interface that declares it. Methods of the class come first,
    /// so they win over the interfaces' defaults.
    pub fn find_method(&self, name: &str) -> Option<(MemberSig, ClassRef)> {
        if let Some(found) = self.find(|b| b.methods.get(name).cloned()) {
            return Some(found);
        }
        let mut class = Some(self.clone());
        while let Some(c) = class {
            let interfaces = c.0.body.borrow().interfaces.clone();
            for i in interfaces {
                if let Some(found) = i.find_method(name) {
                    return Some(found);
                }
            }
            class = c.0.body.borrow().parent.clone();
        }
        None
    }

    /// Every interface it implements, also through its superclasses and the
    /// interfaces they extend.
    pub fn all_interfaces(&self) -> Vec<ClassRef> {
        let mut out: Vec<ClassRef> = Vec::new();
        let body = self.0.body.borrow();
        let mut add = |c: ClassRef| {
            if !out.contains(&c) {
                out.push(c);
            }
        };
        if let Some(p) = &body.parent {
            for i in p.all_interfaces() {
                add(i);
            }
        }
        for i in &body.interfaces {
            add(i.clone());
            for j in i.all_interfaces() {
                add(j);
            }
        }
        out
    }

    /// Finds a member in the class or its superclasses, with the class that declares it.
    pub fn find<T: Clone>(&self, get: impl Fn(&ClassBody) -> Option<T>) -> Option<(T, ClassRef)> {
        let mut class = Some(self.clone());
        while let Some(c) = class {
            if let Some(found) = get(&c.0.body.borrow()) {
                return Some((found, c.clone()));
            }
            class = c.0.body.borrow().parent.clone();
        }
        None
    }
}

pub struct ClassSig {
    pub name: Rc<str>,
    pub interface: bool,
    pub is_enum: bool,
    /// A command or a subcommand, whose objects only come from `parse()`.
    pub command: bool,
    pub generics: Vec<Rc<str>>,
    /// Filled once every class of the block has a name, so they can use each other.
    pub body: RefCell<ClassBody>,
    /// Where it and its members are declared, for editors.
    pub places: RefCell<Places>,
}

/// Where a class and its members are declared *(see `checker/names.rs`)*.
#[derive(Default, Clone)]
pub struct Places {
    /// `None` for the script being checked.
    pub file: Option<String>,
    pub class: Option<crate::token::Span>,
    /// By name: `name` for fields and methods, `$name` for static fields,
    /// `::name` for static methods and enum values, `new` for the constructor.
    pub members: HashMap<String, crate::token::Span>,
}

#[derive(Default)]
pub struct ClassBody {
    pub parent: Option<ClassRef>,
    /// The built-in exception it extends, directly or through its superclasses.
    pub exception: Option<Rc<str>>,
    /// The types given to the superclass's type parameters *(i.e. `number` in `extends Stack<number>`)*.
    pub parent_args: Vec<Ty>,
    /// The interfaces it implements, or the ones an interface extends.
    pub interfaces: Vec<ClassRef>,
    pub interface_args: Vec<Vec<Ty>>,
    /// Methods with the same name in several `implement` blocks *(i.e. `mul`
    /// of `IMul<number>` and of `IMul<Vector>`)*: every version. The one in
    /// `methods` is the first.
    pub overloads: HashMap<String, Vec<MemberSig>>,
    pub fields: Vec<FieldSig>,
    pub methods: HashMap<String, MemberSig>,
    pub statics: HashMap<String, FieldSig>,
    pub static_methods: HashMap<String, MemberSig>,
    /// What calling the class takes: its `new`, or every field.
    pub constructor: Option<Rc<FnTy>>,
    /// Whether it has its own `new`.
    pub own_constructor: bool,
    /// The values of an enum, in order.
    pub values: Vec<String>,
    /// For the type of a command's `subcommand` field: the subcommands it can be.
    pub subcommands: Vec<ClassRef>,
}

#[derive(Clone)]
pub struct FieldSig {
    pub name: String,
    pub ty: Ty,
    pub public: bool,
    pub constant: bool,
    pub has_value: bool,
}

#[derive(Clone)]
pub struct MemberSig {
    pub ty: Rc<FnTy>,
    pub public: bool,
    /// `false` for a method of an interface without a default.
    pub has_body: bool,
}

#[derive(Debug, PartialEq)]
pub struct FnTy {
    /// For a variadic function, the last one is the type of each extra argument.
    pub params: Vec<Ty>,
    /// The names of the parameters, for named arguments. Empty if they have none.
    pub names: Vec<String>,
    /// How many parameters must be given: the ones before the first default.
    pub required: usize,
    pub variadic: bool,
    pub ret: Ty,
    /// The type parameters, replaced by new type variables at every call.
    pub generics: Vec<Rc<str>>,
}

impl FnTy {
    /// A function that takes anything and returns anything, for functions
    /// the checker knows nothing about *(i.e. registered by a host program)*.
    pub fn unknown() -> Rc<FnTy> {
        Rc::new(FnTy { params: vec![Ty::Any], names: Vec::new(), required: 0, variadic: true, ret: Ty::Any, generics: Vec::new() })
    }
}

impl Ty {
    pub fn nullable(self) -> Ty {
        match self {
            Ty::Nullable(_) | Ty::Any | Ty::Null => self,
            other => Ty::Nullable(Box::new(other)),
        }
    }

    /// Replaces type parameters *(i.e. `T` with `string`)*.
    pub fn substitute(&self, map: &HashMap<Rc<str>, Ty>) -> Ty {
        match self {
            Ty::Param(name) => map.get(name).cloned().unwrap_or_else(|| self.clone()),
            Ty::Result(a, b) => Ty::Result(Box::new(a.substitute(map)), Box::new(b.substitute(map))),
            Ty::List(a) => Ty::List(Box::new(a.substitute(map))),
            Ty::TypeOf(a) => Ty::TypeOf(Box::new(a.substitute(map))),
            Ty::Map(a, b) => Ty::Map(Box::new(a.substitute(map)), Box::new(b.substitute(map))),
            Ty::Nullable(a) => Ty::Nullable(Box::new(a.substitute(map))),
            Ty::Object(c, args) => Ty::Object(c.clone(), args.iter().map(|a| a.substitute(map)).collect()),
            Ty::Func(f) => Ty::Func(Rc::new(FnTy {
                params: f.params.iter().map(|p| p.substitute(map)).collect(),
                names: f.names.clone(),
                required: f.required,
                variadic: f.variadic,
                ret: f.ret.substitute(map),
                generics: f.generics.iter().filter(|g| !map.contains_key(*g)).cloned().collect(),
            })),
            other => other.clone(),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Any | Ty::Var(_) => write!(f, "any"),
            Ty::Void => write!(f, "void"),
            Ty::Never => write!(f, "never"),
            Ty::Null => write!(f, "null"),
            Ty::Bool => write!(f, "bool"),
            Ty::Number => write!(f, "number"),
            Ty::Str => write!(f, "string"),
            Ty::Regex => write!(f, "regex"),
            Ty::Bytes => write!(f, "bytes"),
            Ty::Range => write!(f, "range"),
            Ty::Command => write!(f, "command"),
            Ty::Exception(kind) => write!(f, "{kind}"),
            Ty::Result(a, b) => write!(f, "result<{a}, {b}>"),
            Ty::List(a) => write!(f, "list<{a}>"),
            Ty::TypeOf(a) => write!(f, "type<{a}>"),
            Ty::Map(a, b) => write!(f, "map<{a}, {b}>"),
            Ty::Nullable(a) => match **a {
                Ty::Func(_) => write!(f, "?({a})"),
                _ => write!(f, "?{a}"),
            },
            Ty::Param(name) => write!(f, "{name}"),
            Ty::Object(class, args) if args.is_empty() => write!(f, "{}", class.0.name),
            Ty::Object(class, args) => {
                let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{}<{}>", class.0.name, args.join(", "))
            }
            Ty::Func(func) => {
                let mut params: Vec<String> = func.params.iter().map(|p| p.to_string()).collect();
                if func.variadic {
                    if let Some(last) = params.last_mut() {
                        *last = format!("...{last}");
                    }
                }
                write!(f, "({}) -> {}", params.join(", "), func.ret)
            }
        }
    }
}

/// How a value of this type is described in messages: "a string", "a list<number>".
pub fn a(ty: &Ty) -> String {
    match ty {
        Ty::Any | Ty::Var(_) => "any value".into(),
        Ty::Null => "`null`".into(),
        Ty::Void => "nothing (`void`)".into(),
        Ty::Never => "nothing (`never`)".into(),
        Ty::Bytes => "bytes".into(),
        Ty::TypeOf(t) => format!("the type `{t}`"),
        Ty::Nullable(_) => format!("a {ty}, which can be `null`"),
        other => {
            let text = other.to_string();
            let article = if text.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
            format!("{article} {text}")
        }
    }
}
