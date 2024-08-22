use ruff_text_size::{TextSize, TextRange};
use serde_json::{Value, json};
use tracing::{info, trace};
use weak_table::traits::WeakElement;

use crate::constants::*;
use crate::core::evaluation::{Context, Evaluation};
use crate::core::odoo::SyncOdoo;
use crate::core::python_arch_eval::PythonArchEval;
use crate::threads::SessionInfo;
use crate::utils::{MaxTextSize, PathSanitizer as _};
use crate::S;
use core::panic;
use std::collections::{HashMap, VecDeque};
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::{u32, vec};
use lsp_types::Diagnostic;

use crate::core::symbols::function_symbol::FunctionSymbol;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::core::symbols::root_symbol::RootSymbol;

use super::class_symbol::ClassSymbol;
use super::compiled_symbol::CompiledSymbol;
use super::file_symbol::FileSymbol;
use super::namespace_symbol::{NamespaceDirectory, NamespaceSymbol};
use super::package_symbol::PackageSymbol;
use super::symbol_mgr::SymbolMgr;
use super::variable_symbol::VariableSymbol;

#[derive(Debug)]
pub enum Symbol {
    Root(RootSymbol),
    Namespace(NamespaceSymbol),
    Package(PackageSymbol),
    File(FileSymbol),
    Compiled(CompiledSymbol),
    Class(ClassSymbol),
    Function(FunctionSymbol),
    Variable(VariableSymbol),
}

impl Symbol {
    pub fn new_root() -> Rc<RefCell<Self>> {
        let root = Rc::new(RefCell::new(Symbol::Root(RootSymbol::new())));
        root.borrow_mut().set_weak_self(Rc::downgrade(&root));
        root
    }

    //Create a sub-symbol that is representing a file
    pub fn add_new_file(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let file = Rc::new(RefCell::new(Symbol::File(FileSymbol::new(name.clone(), path.clone(), self.is_external()))));
        file.borrow_mut().set_weak_self(Rc::downgrade(&file));
        file.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&file);
            },
            Symbol::Package(p) => {
                p.add_file(&file);
            },
            Symbol::Root(r) => {
                r.add_file(session, &file);
            }
            _ => { panic!("Impossible to add a file to a {}", self.typ()); }
        }
        file
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_python_package(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let package = Rc::new(
            RefCell::new(
                Symbol::Package(
                    PackageSymbol::new_python_package(name.clone(), path.clone(), self.is_external())
                )
            )
        );
        package.borrow_mut().set_weak_self(Rc::downgrade(&package));
        package.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&package);
            },
            Symbol::Package(p) => {
                p.add_file(&package);
            },
            Symbol::Root(r) => {
                r.add_file(session, &package)
            }
            _ => { panic!("Impossible to add a package to a {}", self.typ()); }
        }
        package
    }

    //Create a sub-symbol that is representing a package
    pub fn add_new_module_package(&mut self, session: &mut SessionInfo, name: &String, path: &PathBuf) -> Option<Rc<RefCell<Self>>> {
        let module = PackageSymbol::new_module_package(session, name.clone(), path, self.is_external());
        if module.is_none() {
            return None;
        }
        let module = module.unwrap();
        let package = Rc::new(
            RefCell::new(
                Symbol::Package(
                    module
                )
            )
        );
        package.borrow_mut().set_weak_self(Rc::downgrade(&package));
        package.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&package);
            },
            Symbol::Package(p) => {
                p.add_file(&package);
            },
            Symbol::Root(r) => {
                r.add_file(session, &package)
            }
            _ => { panic!("Impossible to add a package to a {}", self.typ()); }
        }
        Some(package)
    }

    pub fn add_new_namespace(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let namespace = Rc::new(RefCell::new(Symbol::Namespace(NamespaceSymbol::new(name.clone(), vec![path.clone()], self.is_external()))));
        namespace.borrow_mut().set_weak_self(Rc::downgrade(&namespace));
        namespace.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&namespace);
            },
            Symbol::Package(p) => {
                p.add_file(&namespace);
            },
            Symbol::Root(r) => {
                r.add_file(session, &namespace);
            }
            _ => { panic!("Impossible to add a namespace to a {}", self.typ()); }
        }
        namespace
    }

    pub fn add_new_compiled(&mut self, session: &mut SessionInfo, name: &String, path: &String) -> Rc<RefCell<Self>> {
        let compiled = Rc::new(RefCell::new(Symbol::Compiled(CompiledSymbol::new(name.clone(), path.clone(), self.is_external()))));
        compiled.borrow_mut().set_weak_self(Rc::downgrade(&compiled));
        compiled.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::Namespace(n) => {
                n.add_file(&compiled);
            },
            Symbol::Package(p) => {
                p.add_file(&compiled);
            },
            Symbol::Root(r) => {
                r.add_file(session, &compiled);
            },
            Symbol::Compiled(c) => {
                c.add_compiled(&compiled);
            }
            _ => { panic!("Impossible to add a compiled to a {}", self.typ()); }
        }
        compiled
    }

    pub fn add_new_variable(&mut self, session: &mut SessionInfo, name: &String, range: &TextRange) -> Rc<RefCell<Self>> {
        let variable = Rc::new(RefCell::new(Symbol::Variable(VariableSymbol::new(name.clone(), range.clone(), self.is_external()))));
        variable.borrow_mut().set_weak_self(Rc::downgrade(&variable));
        variable.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&variable, section);
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(&variable, section);
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(&variable, section);
            },
            Symbol::Class(c) => {
                let section = c.get_section_for(range.start().to_u32()).index;
                c.add_symbol(&variable, section);
            },
            Symbol::Function(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&variable, section);
            }
            _ => { panic!("Impossible to add a variable to a {}", self.typ()); }
        }
        variable
    }

    pub fn add_new_function(&mut self, session: &mut SessionInfo, name: &String, range: &TextRange) -> Rc<RefCell<Self>> {
        let function = Rc::new(RefCell::new(Symbol::Function(FunctionSymbol::new(name.clone(), range.clone(), self.is_external()))));
        function.borrow_mut().set_weak_self(Rc::downgrade(&function));
        function.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&function, section);
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(&function, section);
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(&function, section);
            },
            Symbol::Class(c) => {
                let section = c.get_section_for(range.start().to_u32()).index;
                c.add_symbol(&function, section);
            }
            Symbol::Function(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&function, section);
            }
            _ => { panic!("Impossible to add a function to a {}", self.typ()); }
        }
        function
    }

    pub fn add_new_class(&mut self, session: &mut SessionInfo, name: &String, range: &TextRange) -> Rc<RefCell<Self>> {
        let class = Rc::new(RefCell::new(Symbol::Class(ClassSymbol::new(name.clone(), range.clone(), self.is_external()))));
        class.borrow_mut().set_weak_self(Rc::downgrade(&class));
        class.borrow_mut().set_parent(Some(self.weak_self().unwrap()));
        match self {
            Symbol::File(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&class, section);
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                let section = m.get_section_for(range.start().to_u32()).index;
                m.add_symbol(&class, section);
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                let section = p.get_section_for(range.start().to_u32()).index;
                p.add_symbol(&class, section);
            },
            Symbol::Class(c) => {
                let section = c.get_section_for(range.start().to_u32()).index;
                c.add_symbol(&class, section);
            }
            Symbol::Function(f) => {
                let section = f.get_section_for(range.start().to_u32()).index;
                f.add_symbol(&class, section);
            }
            _ => { panic!("Impossible to add a class to a {}", self.typ()); }
        }
        class
    }

    pub fn as_root(&self) -> &RootSymbol {
        match self {
            Symbol::Root(r) => r,
            _ => {panic!("Not a Root")}
        }
    }
    pub fn as_root_mut(&mut self) -> &mut RootSymbol {
        match self {
            Symbol::Root(r) => r,
            _ => {panic!("Not a Root")}
        }
    }
    pub fn as_file(&self) -> &FileSymbol {
        match self {
            Symbol::File(f) => f,
            _ => {panic!("Not a File")}
        }
    }
    pub fn as_file_mut(&mut self) -> &mut FileSymbol {
        match self {
            Symbol::File(f) => f,
            _ => {panic!("Not a File")}
        }
    }
    pub fn as_package(&self) -> &PackageSymbol {
        match self {
            Symbol::Package(p) => p,
            _ => {panic!("Not a package")}
        }
    }
    pub fn as_package_mut(&mut self) -> &mut PackageSymbol {
        match self {
            Symbol::Package(p) => p,
            _ => {panic!("Not a package")}
        }
    }
    pub fn as_module_package(&self) -> &ModuleSymbol {
        match self {
            Symbol::Package(PackageSymbol::Module(m)) => m,
            _ => {panic!("Not a module package")}
        }
    }
    pub fn as_module_package_mut(&mut self) -> &mut ModuleSymbol {
        match self {
            Symbol::Package(PackageSymbol::Module(m)) => m,
            _ => {panic!("Not a module package")}
        }
    }

    pub fn as_variable(&self) -> &VariableSymbol {
        match self {
            Symbol::Variable(v) => v,
            _ => {panic!("Not a variable")}
        }
    }

    pub fn as_variable_mut(&mut self) -> &mut VariableSymbol {
        match self {
            Symbol::Variable(v) => v,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_func(&self) -> &FunctionSymbol {
        match self {
            Symbol::Function(f) => f,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_func_mut(&mut self) -> &mut FunctionSymbol {
        match self {
            Symbol::Function(f) => f,
            _ => {panic!("Not a function")}
        }
    }

    pub fn as_class_sym(&self) -> &ClassSymbol {
        match self {
            Symbol::Class(c) => c,
            _ => {panic!("Not a class")}
        }
    }

    pub fn as_class_sym_mut(&mut self) -> &mut ClassSymbol {
        match self {
            Symbol::Class(c) => c,
            _ => {panic!("Not a class")}
        }
    }

    pub fn as_symbol_mgr(&self) -> &dyn SymbolMgr {
        match self {
            Symbol::File(f) => f,
            Symbol::Class(c) => c,
            Symbol::Function(f) => f,
            Symbol::Package(PackageSymbol::Module(m)) => m,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p,
            _ => {panic!("Not a symbol Mgr");}
        }
    }

    pub fn typ(&self) -> SymType {
        match self {
            Symbol::Root(_) => SymType::ROOT,
            Symbol::Namespace(_) => SymType::NAMESPACE,
            Symbol::Package(_) => SymType::PACKAGE,
            Symbol::File(_) => SymType::FILE,
            Symbol::Compiled(_) => SymType::COMPILED,
            Symbol::Class(_) => SymType::CLASS,
            Symbol::Function(_) => SymType::FUNCTION,
            Symbol::Variable(_) => SymType::VARIABLE,
        }
    }

    pub fn name(&self) -> &String {
        match self {
            Symbol::Root(r) => &r.name,
            Symbol::Namespace(n) => &n.name,
            Symbol::Package(p) => &p.name(),
            Symbol::File(f) => &f.name,
            Symbol::Compiled(c) => &c.name,
            Symbol::Class(c) => &c.name,
            Symbol::Function(f) => &f.name,
            Symbol::Variable(v) => &v.name,
        }
    }

    pub fn doc_string(&self) -> &Option<String> {
        match self {
            Symbol::Root(r) => &None,
            Symbol::Namespace(n) => &None,
            Symbol::Package(p) => &None,
            Symbol::File(f) => &None,
            Symbol::Compiled(c) => &None,
            Symbol::Class(c) => &c.doc_string,
            Symbol::Function(f) => &f.doc_string,
            Symbol::Variable(v) => &v.doc_string,
        }
    }

    pub fn set_doc_string(&mut self, doc_string: Option<String>) {
        match self {
            Symbol::Root(r) => panic!(),
            Symbol::Namespace(n) => panic!(),
            Symbol::Package(p) => panic!(),
            Symbol::File(f) => panic!(),
            Symbol::Compiled(c) => panic!(),
            Symbol::Class(c) => c.doc_string = doc_string,
            Symbol::Function(f) => f.doc_string = doc_string,
            Symbol::Variable(v) => v.doc_string = doc_string,
        }
    }

    pub fn is_external(&self) -> bool {
        match self {
            Symbol::Root(r) => false,
            Symbol::Namespace(n) => n.is_external,
            Symbol::Package(p) => p.is_external(),
            Symbol::File(f) => f.is_external,
            Symbol::Compiled(c) => c.is_external,
            Symbol::Class(c) => c.is_external,
            Symbol::Function(f) => f.is_external,
            Symbol::Variable(v) => v.is_external,
        }
    }
    pub fn set_is_external(&mut self, external: bool) {
        match self {
            Symbol::Root(r) => {},
            Symbol::Namespace(n) => n.is_external = external,
            Symbol::Package(PackageSymbol::Module(m)) => m.is_external = external,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.is_external = external,
            Symbol::File(f) => f.is_external = external,
            Symbol::Compiled(c) => c.is_external = external,
            Symbol::Class(c) => c.is_external = external,
            Symbol::Function(f) => f.is_external = external,
            Symbol::Variable(v) => v.is_external = external,
        }
    }

    pub fn range(&self) -> &TextRange {
        match self {
            Symbol::Root(r) => panic!(),
            Symbol::Namespace(n) => panic!(),
            Symbol::Package(p) => panic!(),
            Symbol::File(f) => panic!(),
            Symbol::Compiled(c) => panic!(),
            Symbol::Class(c) => &c.range,
            Symbol::Function(f) => &f.range,
            Symbol::Variable(v) => &v.range,
        }
    }

    pub fn has_ast_indexes(&self) -> bool {
        match self {
            Symbol::Variable(v) => true,
            Symbol::Class(c) => true,
            Symbol::Function(f) => true,
            Symbol::File(f) => false,
            Symbol::Compiled(c) => false,
            Symbol::Namespace(n) => false,
            Symbol::Package(p) => false,
            Symbol::Root(r) => false,
        }
    }

    pub fn ast_indexes(&self) -> Option<&Vec<u16>> {
        match self {
            Symbol::Variable(v) => Some(&v.ast_indexes),
            Symbol::Class(c) => Some(&c.ast_indexes),
            Symbol::Function(f) => Some(&f.ast_indexes),
            Symbol::File(f) => None,
            Symbol::Compiled(c) => None,
            Symbol::Namespace(n) => None,
            Symbol::Package(p) => None,
            Symbol::Root(r) => None,
        }
    }

    pub fn ast_indexes_mut(&mut self) -> &mut Vec<u16> {
        match self {
            Symbol::Variable(v) => &mut v.ast_indexes,
            Symbol::Class(c) => &mut c.ast_indexes,
            Symbol::Function(f) => &mut f.ast_indexes,
            Symbol::File(f) => panic!(),
            Symbol::Compiled(c) => panic!(),
            Symbol::Namespace(n) => panic!(),
            Symbol::Package(p) => panic!(),
            Symbol::Root(r) => panic!(),
        }
    }

    pub fn weak_self(&self) -> Option<Weak<RefCell<Symbol>>> {
        match self {
            Symbol::Root(r) => r.weak_self.clone(),
            Symbol::Namespace(n) => n.weak_self.clone(),
            Symbol::Package(PackageSymbol::Module(m)) => m.weak_self.clone(),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.weak_self.clone(),
            Symbol::File(f) => f.weak_self.clone(),
            Symbol::Compiled(c) => c.weak_self.clone(),
            Symbol::Class(c) => c.weak_self.clone(),
            Symbol::Function(f) => f.weak_self.clone(),
            Symbol::Variable(v) => v.weak_self.clone(),
        }
    }

    pub fn parent(&self) -> Option<Weak<RefCell<Symbol>>> {
        match self {
            Symbol::Root(r) => r.parent.clone(),
            Symbol::Namespace(n) => n.parent.clone(),
            Symbol::Package(p) => p.parent(),
            Symbol::File(f) => f.parent.clone(),
            Symbol::Compiled(c) => c.parent.clone(),
            Symbol::Class(c) => c.parent.clone(),
            Symbol::Function(f) => f.parent.clone(),
            Symbol::Variable(v) => v.parent.clone(),
        }
    }

    fn set_parent(&mut self, parent: Option<Weak<RefCell<Symbol>>>) {
        match self {
            Symbol::Root(r) => panic!(),
            Symbol::Namespace(n) => n.parent = parent,
            Symbol::Package(p) => p.set_parent(parent),
            Symbol::File(f) => f.parent = parent,
            Symbol::Compiled(c) => c.parent = parent,
            Symbol::Class(c) => c.parent = parent,
            Symbol::Function(f) => f.parent = parent,
            Symbol::Variable(v) => v.parent = parent,
        }
    }
    
    pub fn paths(&self) -> Vec<String> {
        match self {
            Symbol::Root(r) => r.paths.clone(),
            Symbol::Namespace(n) => n.paths(),
            Symbol::Package(p) => p.paths(),
            Symbol::File(f) => vec![f.path.clone()],
            Symbol::Compiled(c) => vec![c.path.clone()],
            Symbol::Class(c) => vec![],
            Symbol::Function(f) => vec![],
            Symbol::Variable(v) => vec![],
        }
    }
    pub fn add_path(&mut self, path: String) {
        match self {
            Symbol::Root(r) => r.paths.push(path),
            Symbol::Namespace(n) => {
                n.directories.push(NamespaceDirectory { path: path, module_symbols: HashMap::new() });
            },
            Symbol::Package(p) => {},
            Symbol::File(f) => {},
            Symbol::Compiled(c) => {},
            Symbol::Class(c) => {},
            Symbol::Function(f) => {},
            Symbol::Variable(v) => {},
        }
    }

    pub fn dependencies(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4] {
        match self {
            Symbol::Root(r) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &n.dependencies,
            Symbol::Package(p) => p.dependencies(),
            Symbol::File(f) => &f.dependencies,
            Symbol::Compiled(c) => panic!("No dependencies on Compiled"),
            Symbol::Class(c) => panic!("No dependencies on Class"),
            Symbol::Function(f) => panic!("No dependencies on Function"),
            Symbol::Variable(v) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependencies_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4] {
        match self {
            Symbol::Root(r) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &mut n.dependencies,
            Symbol::Package(p) => p.dependencies_as_mut(),
            Symbol::File(f) => &mut f.dependencies,
            Symbol::Compiled(c) => panic!("No dependencies on Compiled"),
            Symbol::Class(c) => panic!("No dependencies on Class"),
            Symbol::Function(f) => panic!("No dependencies on Function"),
            Symbol::Variable(v) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependents(&self) -> &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3] {
        match self {
            Symbol::Root(r) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &n.dependents,
            Symbol::Package(p) => p.dependents(),
            Symbol::File(f) => &f.dependents,
            Symbol::Compiled(c) => panic!("No dependencies on Compiled"),
            Symbol::Class(c) => panic!("No dependencies on Class"),
            Symbol::Function(f) => panic!("No dependencies on Function"),
            Symbol::Variable(v) => panic!("No dependencies on Variable"),
        }
    }
    pub fn dependents_as_mut(&mut self) -> &mut [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3] {
        match self {
            Symbol::Root(r) => panic!("No dependencies on Root"),
            Symbol::Namespace(n) => &mut n.dependents,
            Symbol::Package(p) => p.dependents_as_mut(),
            Symbol::File(f) => &mut f.dependents,
            Symbol::Compiled(c) => panic!("No dependencies on Compiled"),
            Symbol::Class(c) => panic!("No dependencies on Class"),
            Symbol::Function(f) => panic!("No dependencies on Function"),
            Symbol::Variable(v) => panic!("No dependencies on Variable"),
        }
    }
    pub fn has_modules(&self) -> bool {
        match self {
            Symbol::Root(_) | Symbol::Namespace(_) | Symbol::Package(_) => true,
            _ => {false}
        }
    }
    pub fn all_module_symbol(&self) -> Box<dyn Iterator<Item = &Rc<RefCell<Symbol>>> + '_> {
        match self {
            Symbol::Root(r) => Box::new(r.module_symbols.values()),
            Symbol::Namespace(n) => {
                Box::new(n.directories.iter().flat_map(|x| x.module_symbols.values()))
            },
            Symbol::Package(PackageSymbol::Module(m)) => Box::new(m.module_symbols.values()),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => Box::new(p.module_symbols.values()),
            Symbol::File(f) => panic!("No module symbol on File"),
            Symbol::Compiled(c) => panic!("No module symbol on Compiled"),
            Symbol::Class(c) => panic!("No module symbol on Class"),
            Symbol::Function(f) => panic!("No module symbol on Function"),
            Symbol::Variable(v) => panic!("No module symbol on Variable"),
        }
    }
    pub fn in_workspace(&self) -> bool {
        match self {
            Symbol::Root(r) => false,
            Symbol::Namespace(n) => n.in_workspace,
            Symbol::Package(PackageSymbol::Module(m)) => m.in_workspace,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.in_workspace,
            Symbol::File(f) => f.in_workspace,
            Symbol::Compiled(c) => panic!(),
            Symbol::Class(c) => panic!(),
            Symbol::Function(f) => panic!(),
            Symbol::Variable(v) => panic!(),
        }
    }
    pub fn set_in_workspace(&mut self, in_workspace: bool) {
        match self {
            Symbol::Root(r) => panic!(),
            Symbol::Namespace(n) => n.in_workspace = in_workspace,
            Symbol::Package(PackageSymbol::Module(m)) => m.in_workspace = in_workspace,
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.in_workspace = in_workspace,
            Symbol::File(f) => f.in_workspace = in_workspace,
            Symbol::Compiled(c) => panic!(),
            Symbol::Class(c) => panic!(),
            Symbol::Function(f) => panic!(),
            Symbol::Variable(v) => panic!(),
        }
    }
    pub fn build_status(&self, step:BuildSteps) -> BuildStatus {
        match self {
            Symbol::Root(r) => {panic!()},
            Symbol::Namespace(n) => {panic!()},
            Symbol::Package(PackageSymbol::Module(m)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => m.arch_status,
                    BuildSteps::ARCH_EVAL => m.arch_eval_status,
                    BuildSteps::ODOO => m.odoo_status,
                    BuildSteps::VALIDATION => m.validation_status,
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status,
                    BuildSteps::ODOO => p.odoo_status,
                    BuildSteps::VALIDATION => p.validation_status,
                }
            }
            Symbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status,
                    BuildSteps::ODOO => f.odoo_status,
                    BuildSteps::VALIDATION => f.validation_status,
                }
            },
            Symbol::Compiled(_) => todo!(),
            Symbol::Class(c) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => c.arch_status,
                    BuildSteps::ARCH_EVAL => c.arch_eval_status,
                    BuildSteps::ODOO => c.odoo_status,
                    BuildSteps::VALIDATION => c.validation_status,
                }
            },
            Symbol::Function(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status,
                    BuildSteps::ODOO => f.odoo_status,
                    BuildSteps::VALIDATION => f.validation_status,
                }
            },
            Symbol::Variable(_) => todo!(),
        }
    }
    pub fn set_build_status(&mut self, step:BuildSteps, status: BuildStatus) {
        match self {
            Symbol::Root(r) => {panic!()},
            Symbol::Namespace(n) => {panic!()},
            Symbol::Package(PackageSymbol::Module(m)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => m.arch_status = status,
                    BuildSteps::ARCH_EVAL => m.arch_eval_status = status,
                    BuildSteps::ODOO => m.odoo_status = status,
                    BuildSteps::VALIDATION => m.validation_status = status,
                }
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => p.arch_status = status,
                    BuildSteps::ARCH_EVAL => p.arch_eval_status = status,
                    BuildSteps::ODOO => p.odoo_status = status,
                    BuildSteps::VALIDATION => p.validation_status = status,
                }
            }
            Symbol::File(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status = status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status = status,
                    BuildSteps::ODOO => f.odoo_status = status,
                    BuildSteps::VALIDATION => f.validation_status = status,
                }
            },
            Symbol::Compiled(_) => panic!(),
            Symbol::Class(c) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => c.arch_status = status,
                    BuildSteps::ARCH_EVAL => c.arch_eval_status = status,
                    BuildSteps::ODOO => c.odoo_status = status,
                    BuildSteps::VALIDATION => c.validation_status = status,
                }
            },
            Symbol::Function(f) => {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => f.arch_status = status,
                    BuildSteps::ARCH_EVAL => f.arch_eval_status = status,
                    BuildSteps::ODOO => f.odoo_status = status,
                    BuildSteps::VALIDATION => f.validation_status = status,
                }
            },
            Symbol::Variable(_) => todo!(),
        }
    }

    pub fn iter_symbols(&self) -> std::collections::hash_map::Iter<String, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>> {
        match self {
            Symbol::File(f) => {
                f.symbols.iter()
            }
            Symbol::Root(r) => panic!(),
            Symbol::Namespace(n) => panic!(),
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.symbols.iter()
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.symbols.iter()
            }
            Symbol::Compiled(c) => panic!(),
            Symbol::Class(c) => {
                c.symbols.iter()
            },
            Symbol::Function(f) => {
                f.symbols.iter()
            },
            Symbol::Variable(v) => panic!(),
        }
    }
    pub fn evaluations(&self) -> Option<&Vec<Evaluation>> {
        match self {
            Symbol::File(f) => { None },
            Symbol::Root(r) => { None },
            Symbol::Namespace(n) => { None },
            Symbol::Package(p) => { None },
            Symbol::Compiled(c) => { None },
            Symbol::Class(c) => { None },
            Symbol::Function(f) => Some(&f.evaluations),
            Symbol::Variable(v) => Some(&v.evaluations),
        }
    }
    pub fn evaluations_mut(&mut self) -> Option<&mut Vec<Evaluation>> {
        match self {
            Symbol::File(f) => { None },
            Symbol::Root(r) => { None },
            Symbol::Namespace(n) => { None },
            Symbol::Package(p) => { None },
            Symbol::Compiled(c) => { None },
            Symbol::Class(c) => { None },
            Symbol::Function(f) => Some(&mut f.evaluations),
            Symbol::Variable(v) => Some(&mut v.evaluations),
        }
    }
    pub fn set_evaluations(&mut self, data: Vec<Evaluation>) {
        match self {
            Symbol::File(f) => { panic!() },
            Symbol::Root(r) => { panic!() },
            Symbol::Namespace(n) => { panic!() },
            Symbol::Package(p) => { panic!() },
            Symbol::Compiled(c) => { panic!() },
            Symbol::Class(c) => { panic!() },
            Symbol::Function(f) => { f.evaluations = data; },
            Symbol::Variable(v) => v.evaluations = data,
        }
    }

    pub fn not_found_paths(&self) -> &Vec<(BuildSteps, Vec<String>)> {
        static EMPTY_VEC: Vec<(BuildSteps, Vec<String>)> = Vec::new();
        match self {
            Symbol::File(f) => { &f.not_found_paths },
            Symbol::Root(r) => { &EMPTY_VEC },
            Symbol::Namespace(n) => { &EMPTY_VEC },
            Symbol::Package(PackageSymbol::Module(m)) => { &m.not_found_paths },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => { &p.not_found_paths },
            Symbol::Compiled(c) => { &EMPTY_VEC },
            Symbol::Class(c) => { &EMPTY_VEC },
            Symbol::Function(f) => { &EMPTY_VEC },
            Symbol::Variable(v) => &EMPTY_VEC,
        }
    }

    pub fn not_found_paths_mut(&mut self) -> &mut Vec<(BuildSteps, Vec<String>)> {
        match self {
            Symbol::File(f) => { &mut f.not_found_paths },
            Symbol::Root(r) => { panic!("no not_found_path on Root") },
            Symbol::Namespace(n) => { panic!("no not_found_path on Namespace") },
            Symbol::Package(PackageSymbol::Module(m)) => { &mut m.not_found_paths },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => { &mut p.not_found_paths },
            Symbol::Compiled(c) => { panic!("no not_found_path on Compiled") },
            Symbol::Class(c) => { panic!("no not_found_path on Class") },
            Symbol::Function(f) => { panic!("no not_found_path on Function") },
            Symbol::Variable(v) => panic!("no not_found_path on Variable"),
        }
    }

    ///Given a path, create the appropriated symbol and attach it to the given parent
    pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: Rc<RefCell<Symbol>>, require_module: bool) -> Option<Rc<RefCell<Symbol>>> {
        let name: String = path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let path_str = path.sanitize();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") {
            let ref_sym = (*parent).borrow_mut().add_new_file(session, &name, &path_str);
            return Some(ref_sym);
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                if (*parent).borrow().get_tree().clone() == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
                    let module = (*parent).borrow_mut().add_new_module_package(session, &name, path);
                    if module.is_none() {
                        if require_module {
                            return None;
                        } else {
                            let ref_sym = (*parent).borrow_mut().add_new_python_package(session, &name, &path_str);
                            if !path.join("__init__.py").exists() {
                                (*ref_sym).borrow_mut().as_package_mut().set_i_ext("i".to_string());
                            }
                            return Some(ref_sym);
                        }
                    } else {
                        let module = module.unwrap();
                        ModuleSymbol::load_module_info(module.clone(), session, parent);
                        //as the symbol has been added to parent before module creation, it has not been added to modules
                        session.sync_odoo.modules.insert(module.borrow().as_module_package().dir_name.clone(), Rc::downgrade(&module));
                        return Some(module);
                    }
                } else if require_module {
                    return None;
                } else {
                    if (*parent).borrow().get_tree().clone() == tree(vec!["odoo"], vec![]) && path_str.ends_with("addons") {
                        //Force namespace for odoo/addons
                        let ref_sym = (*parent).borrow_mut().add_new_namespace(session, &name, &path_str);
                        return Some(ref_sym);
                    } else {
                        let ref_sym = (*parent).borrow_mut().add_new_python_package(session, &name, &path_str);
                        if !path.join("__init__.py").exists() {
                            (*ref_sym).borrow_mut().as_package_mut().set_i_ext("i".to_string());
                        }
                        return Some(ref_sym);
                    }
                }
            } else if !require_module{ //TODO should handle module with only __manifest__.py (see odoo/addons/test_data-module)
                let ref_sym = (*parent).borrow_mut().add_new_namespace(session, &name, &path_str);
                return Some(ref_sym);
            } else {
                return None
            }
        }
    }

    pub fn get_tree(&self) -> Tree {
        let mut res = (vec![], vec![]);
        if self.is_file_content() {
            res.1.insert(0, self.name().clone());
        } else {
            res.0.insert(0, self.name().clone());
        }
        if self.typ() == SymType::ROOT || self.parent().is_none() {
            return res
        }
        let parent = self.parent().clone();
        let mut current_arc = parent.as_ref().unwrap().upgrade().unwrap();
        let mut current = current_arc.borrow_mut();
        while current.typ() != SymType::ROOT && current.parent().is_some() {
            if current.is_file_content() {
                res.1.insert(0, current.name().clone());
            } else {
                res.0.insert(0, current.name().clone());
            }
            let parent = current.parent().clone();
            drop(current);
            current_arc = parent.as_ref().unwrap().upgrade().unwrap();
            current = current_arc.borrow_mut();
        }
        res
    }

    pub fn get_symbol(&self, tree: &Tree, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        let symbol_tree_files: &Vec<String> = &tree.0;
        let symbol_tree_content: &Vec<String> = &tree.1;
        let mut iter_sym: Vec<Rc<RefCell<Symbol>>> = vec![];
        if symbol_tree_files.len() != 0 {
            let _mod_iter_sym = self.get_module_symbol(&symbol_tree_files[0]);
            if _mod_iter_sym.is_none() {
                return vec![];
            }
            iter_sym = vec![_mod_iter_sym.unwrap()];
            if symbol_tree_files.len() > 1 {
                for fk in symbol_tree_files[1..symbol_tree_files.len()].iter() {
                    if let Some(s) = iter_sym.last().unwrap().clone().borrow_mut().get_module_symbol(fk) {
                        iter_sym = vec![s.clone()];
                    } else {
                        return vec![];
                    }
                }
            }
            if symbol_tree_content.len() != 0 {
                for fk in symbol_tree_content.iter() {
                    if iter_sym.len() > 1 {
                        trace!("TODO: explore all implementation possibilities");
                    }
                    let _iter_sym = iter_sym[0].borrow_mut().get_content_symbol(fk, position);
                    iter_sym = _iter_sym;
                    if iter_sym.is_empty() {
                        return vec![];
                    }
                }
            }
        } else {
            if symbol_tree_content.len() == 0 {
                return vec![];
            }
            iter_sym = self.get_content_symbol(&symbol_tree_content[0], position);
            if iter_sym.is_empty() {
                return vec![];
            }
            if symbol_tree_content.len() > 1 {
                if iter_sym.len() > 1 {
                    trace!("TODO: explore all implementation possibilities");
                }
                for fk in symbol_tree_content[1..symbol_tree_content.len()].iter() {
                    let _iter_sym = iter_sym[0].borrow_mut().get_content_symbol(fk, position);
                    iter_sym = _iter_sym;
                    return iter_sym.clone();
                }
            }
        }
        iter_sym
    }

    pub fn get_module_symbol(&self, name: &str) -> Option<Rc<RefCell<Symbol>>> {
        match self {
            Symbol::Namespace(n) => {
                for dir in n.directories.iter() {
                    let result = dir.module_symbols.get(name);
                    if let Some(result) = result {
                        return Some(result.clone());
                    }
                }
                None
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.module_symbols.get(name).cloned()
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.module_symbols.get(name).cloned()
            }
            Symbol::Root(r) => {
                r.module_symbols.get(name).cloned()
            }
            _ => {None}
        }
    }

    pub fn get_content_symbol(&self, name: &str, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
        match self {
            Symbol::Class(c) => {
                c.get_symbol(name.to_string(), position)
            },
            Symbol::File(f) => {
                f.get_symbol(name.to_string(), position)
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                m.get_symbol(name.to_string(), position)
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                p.get_symbol(name.to_string(), position)
            },
            Symbol::Function(f) => {
                f.get_symbol(name.to_string(), position)
            },
            _ => {vec![]}
        }
    }

    //Return a HashSet of all symbols (constructed until 'level') that are dependencies for the 'step' of this symbol
    pub fn get_dependencies(&self, step: BuildSteps, level: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<Symbol>>> {
        if step == BuildSteps::SYNTAX || level == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        if level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't get dependencies for step {:?} and level {:?}", step, level)
            }
            if level == BuildSteps::VALIDATION {
                panic!("Can't get dependencies for step {:?} and level {:?}", step, level)
            }
        }
        match self {
            Symbol::Root(r) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => &n.dependencies[step as usize][level as usize],
            Symbol::Package(p) => &p.dependencies()[step as usize][level as usize],
            Symbol::File(f) => &f.dependencies[step as usize][level as usize],
            Symbol::Compiled(c) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(c) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(f) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(v) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    pub fn get_all_dependencies(&self, step: BuildSteps) -> &Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        match self {
            Symbol::Root(r) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => &n.dependencies[step as usize],
            Symbol::Package(p) => &p.dependencies()[step as usize],
            Symbol::File(f) => &f.dependencies[step as usize],
            Symbol::Compiled(c) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(c) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(f) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(v) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    //Return a HashSet of all 'step' of symbols that require that this symbol is built until 'level';
    pub fn get_dependents(&self, level: BuildSteps, step: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<Symbol>>> {
        if level == BuildSteps::SYNTAX || step == BuildSteps::SYNTAX {
            panic!("Can't get dependents for syntax step")
        }
        if level == BuildSteps::VALIDATION {
            panic!("Can't get dependents for level {:?}", level)
        }
        if level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't get dependents for step {:?} and level {:?}", step, level)
            }
        }
        match self {
            Symbol::Root(r) => panic!("There is no dependencies on Root Symbol"),
            Symbol::Namespace(n) => &n.dependents[level as usize][step as usize],
            Symbol::Package(p) => &p.dependents()[level as usize][step as usize],
            Symbol::File(f) => &f.dependents[level as usize][step as usize],
            Symbol::Compiled(c) => panic!("There is no dependencies on Compiled Symbol"),
            Symbol::Class(c) => panic!("There is no dependencies on Class Symbol"),
            Symbol::Function(f) => panic!("There is no dependencies on Function Symbol"),
            Symbol::Variable(v) => panic!("There is no dependencies on Variable Symbol"),
        }
    }

    //Add a symbol as dependency on the step of the other symbol for the build level.
    //-> The build of the 'step' of self requires the build of 'dep_level' of the other symbol to be done
    pub fn add_dependency(&mut self, symbol: &mut Symbol, step:BuildSteps, dep_level:BuildSteps) {
        if step == BuildSteps::SYNTAX || dep_level == BuildSteps::SYNTAX {
            panic!("Can't add dependency for syntax step")
        }
        if dep_level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
            }
            if dep_level == BuildSteps::VALIDATION {
                panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
            }
        }
        if self.typ() != SymType::FILE && self.typ() != SymType::PACKAGE {
            panic!("Dependencies should be only on files");
        }
        if symbol.typ() != SymType::FILE && symbol.typ() != SymType::PACKAGE {
            panic!("Dependencies should be only on files");
        }
        let step_i = step as usize;
        let level_i = dep_level as usize;
        self.dependencies_mut()[step_i][level_i].insert(symbol.get_rc().unwrap());
        symbol.dependents_as_mut()[level_i][step_i].insert(self.get_rc().unwrap());
    }

    pub fn invalidate(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>, step: &BuildSteps) {
        //signals that a change occured to this symbol. "step" indicates which level of change occured.
        //It will trigger rebuild on all dependencies
        let mut vec_to_invalidate: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
            let sym_to_inv = ref_to_inv.borrow();
            if [SymType::FILE, SymType::PACKAGE].contains(&sym_to_inv.typ()) {
                if *step == BuildSteps::ARCH {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ARCH as usize {
                                    session.sync_odoo.add_to_rebuild_arch(sym.clone());
                                } else if index == BuildSteps::ARCH_EVAL as usize {
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL].contains(step) {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ARCH_EVAL as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ARCH_EVAL as usize {
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::ODOO].contains(step) {
                    for (index, hashset) in sym_to_inv.dependents()[BuildSteps::ODOO as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
                                if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
            }
            if sym_to_inv.has_modules() {
                for sym in sym_to_inv.all_module_symbol() {
                    vec_to_invalidate.push_back(sym.clone());
                }
            }
        }
    }

    pub fn unload(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
        let mut vec_to_unload: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while vec_to_unload.len() > 0 {
            let ref_to_unload = vec_to_unload.front().unwrap().clone();
            let mut mut_symbol = ref_to_unload.borrow_mut();
            // Unload children first
            let mut found_one = false;
            for sym in mut_symbol.all_symbols() {
                found_one = true;
                vec_to_unload.push_front(sym.clone());
            }
            if found_one {
                continue;
            } else {
                vec_to_unload.pop_front();
            }
            if DEBUG_MEMORY && (mut_symbol.typ() == SymType::FILE || mut_symbol.typ() == SymType::PACKAGE) {
                info!("Unloading symbol {:?} at {:?}", mut_symbol.name(), mut_symbol.paths());
            }
            //unload symbol
            let parent = mut_symbol.parent().as_ref().unwrap().upgrade().unwrap().clone();
            let mut parent = parent.borrow_mut();
            drop(mut_symbol);
            parent.remove_symbol(ref_to_unload.clone());
            drop(parent);
            if vec![SymType::FILE, SymType::PACKAGE].contains(&ref_to_unload.borrow().typ()) {
                Symbol::invalidate(session, ref_to_unload.clone(), &BuildSteps::ARCH);
            }
            match *ref_to_unload.borrow_mut() {
                Symbol::Package(PackageSymbol::Module(ref mut m)) => {
                    session.sync_odoo.modules.remove(m.dir_name.as_str());
                },
                _ => {}
            }
            drop(ref_to_unload);
        }
    }

    pub fn get_rc(&self) -> Option<Rc<RefCell<Symbol>>> {
        if self.weak_self().is_none() {
            return None;
        }
        if let Some(v) = &self.weak_self() {
            return Some(v.upgrade().unwrap());
        }
        None
    }

    pub fn is_file_content(&self) -> bool{
        match self {
            Symbol::Root(_) | Symbol::Namespace(_) | Symbol::Package(_) | Symbol::File(_) | Symbol::Compiled(_) => false,
            Symbol::Class(_) | Symbol::Function(_) | Symbol::Variable(_) => true
        }
    }

    ///return true if to_test is in parents of symbol or equal to it.
    pub fn is_symbol_in_parents(symbol: &Rc<RefCell<Symbol>>, to_test: &Rc<RefCell<Symbol>>) -> bool {
        if Rc::ptr_eq(symbol, to_test) {
            return true;
        }
        if symbol.borrow().parent().is_none() {
            return false;
        }
        let parent = symbol.borrow().parent().as_ref().unwrap().upgrade().unwrap();
        return Symbol::is_symbol_in_parents(&parent, to_test);
    }

    fn set_weak_self(&mut self, weak_self: Weak<RefCell<Symbol>>) {
        match self {
            Symbol::Root(r) => r.weak_self = Some(weak_self),
            Symbol::Namespace(n) => n.weak_self = Some(weak_self),
            Symbol::Package(PackageSymbol::Module(m)) => m.weak_self = Some(weak_self),
            Symbol::Package(PackageSymbol::PythonPackage(p)) => p.weak_self = Some(weak_self),
            Symbol::File(f) => f.weak_self = Some(weak_self),
            Symbol::Compiled(c) => c.weak_self = Some(weak_self),
            Symbol::Class(c) => c.weak_self = Some(weak_self),
            Symbol::Function(f) => f.weak_self = Some(weak_self),
            Symbol::Variable(v) => v.weak_self = Some(weak_self),
        }
    }

    pub fn get_in_parents(&self, sym_types: &Vec<SymType>, stop_same_file: bool) -> Option<Weak<RefCell<Symbol>>> {
        if sym_types.contains(&self.typ()) {
            return self.weak_self().clone();
        }
        if stop_same_file && vec![SymType::FILE, SymType::PACKAGE].contains(&self.typ()) {
            return None;
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow_mut().get_in_parents(sym_types, stop_same_file);
        }
        return None;
    }

    /// get a Symbol that has the same given range and name
    pub fn get_positioned_symbol(&self, name: &String, range: &TextRange) -> Option<Rc<RefCell<Symbol>>> {
        if let Some(symbols) = match self {
            Symbol::Class(c) => { c.symbols.get(name) },
            Symbol::File(f) => {f.symbols.get(name)},
            Symbol::Function(f) => {f.symbols.get(name)},
            Symbol::Package(PackageSymbol::Module(m)) => {m.symbols.get(name)},
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {p.symbols.get(name)},
            _ => {None}
        } {
            for sym_list in symbols.values() {
                for sym in sym_list.iter() {
                    if sym.borrow().range().start() == range.start() {
                        return Some(sym.clone());
                    }
                }
            }
        }
        None
    }

    pub fn remove_symbol(&mut self, symbol: Rc<RefCell<Symbol>>) {
        if symbol.borrow().is_file_content() {
            match self {
                Symbol::Class(c) => { c.symbols.remove(symbol.borrow().name()); },
                Symbol::File(f) => { f.symbols.remove(symbol.borrow().name()); },
                Symbol::Function(f) => { f.symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::Module(m)) => { m.symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::PythonPackage(p)) => { p.symbols.remove(symbol.borrow().name()); },
                Symbol::Compiled(c) => { panic!("A compiled symbol can not contain python code") },
                Symbol::Namespace(n) => { panic!("A namespace can not contain python code") },
                Symbol::Root(r) => { panic!("Root can not contain python code") },
                Symbol::Variable(v) => { panic!("A variable can not contain python code") }
            };
        } else {
            match self {
                Symbol::Class(c) => { panic!("A class can not contain a file structure") },
                Symbol::File(f) => { panic!("A file can not contain a file structure"); },
                Symbol::Function(f) => { panic!("A function can not contain a file structure") },
                Symbol::Package(PackageSymbol::Module(m)) => { m.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Package(PackageSymbol::PythonPackage(p)) => { p.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Compiled(c) => { c.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Namespace(n) => {
                    for directory in n.directories.iter_mut() {
                        directory.module_symbols.remove(symbol.borrow().name());
                    }
                },
                Symbol::Root(r) => { r.module_symbols.remove(symbol.borrow().name()); },
                Symbol::Variable(v) => { panic!("A variable can not contain a file structure"); }
            };
        }
        symbol.borrow_mut().set_parent(None);
    }

    pub fn get_file(&self) -> Option<Weak<RefCell<Symbol>>> {
        if self.typ() == SymType::FILE || self.typ() == SymType::PACKAGE {
            return self.weak_self().clone();
        }
        if self.parent().is_some() {
            return self.parent().as_ref().unwrap().upgrade().unwrap().borrow_mut().get_file();
        }
        return None;
    }

    pub fn find_module(&self) -> Option<Rc<RefCell<Symbol>>> {
        match self {
            Symbol::Package(PackageSymbol::Module(m)) => {return self.get_rc();}
            _ => {}
        }
        if let Some(parent) = self.parent().as_ref() {
            return parent.upgrade().unwrap().borrow().find_module();
        }
        return None;
    }

    /*given a Symbol, give all the Symbol that are evaluated as valid evaluation for it.
    example:
    ====
    a = 5
    if X:
        a = Test()
    else:
        a = Object()
    print(a)
    ====
    next_refs on the 'a' in the print will return a SymbolRef to Test and one to Object
    */
    pub fn next_refs(session: &mut SessionInfo, symbol: &Symbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> VecDeque<(Weak<RefCell<Symbol>>, bool)> {
        match symbol {
            Symbol::Variable(v) => {
                let mut res = VecDeque::new();
                for eval in v.evaluations.iter() {
                    //TODO context is modified in each for loop, which is wrong !
                    let sym = eval.symbol.get_symbol(session, context, diagnostics);
                    if !sym.0.is_expired() {
                        res.push_back(sym);
                    }
                }
                return res
            },
            _ => {
                let mut vec = VecDeque::new();
                return vec
            }
        }
    }

    pub fn follow_ref(symbol: &Rc<RefCell<Symbol>>, session: &mut SessionInfo, context: &mut Option<Context>, stop_on_type: bool, stop_on_value: bool, diagnostics: &mut Vec<Diagnostic>) -> Vec<(Weak<RefCell<Symbol>>, bool)> {
        //return a list of all possible evaluation: a weak ptr to the final symbol, and a bool indicating if this is an instance or not
        let mut results = Symbol::next_refs(session, &symbol.borrow(), &mut None, &mut vec![]);
        if results.is_empty() {
            return vec![(Rc::downgrade(symbol), symbol.borrow().typ() == SymType::VARIABLE)];
        }
        let can_eval_external = !symbol.borrow().is_external();
        let mut index = 0;
        while index < results.len() {
            let (sym, instance) = &results[index];
            let sym = sym.upgrade();
            if sym.is_none() {
                index += 1;
                continue;
            }
            let sym = sym.unwrap();
            let sym = sym.borrow();
            match *sym {
                Symbol::Variable(ref v) => {
                    if stop_on_type && !instance && !v.is_import_variable {
                        index += 1;
                        continue;
                    }
                    if stop_on_value && v.evaluations.len() == 1 && v.evaluations[0].value.is_some() {
                        index += 1;
                        continue;
                    }
                    if v.evaluations.is_empty() && can_eval_external {
                        //no evaluation? let's check that the file has been evaluated
                        let file_symbol = sym.get_file();
                        match file_symbol {
                            Some(file_symbol) => {
                                if file_symbol.upgrade().expect("invalid weak value").borrow().build_status(BuildSteps::ARCH) == BuildStatus::PENDING &&
                                session.sync_odoo.is_in_rebuild(&file_symbol.upgrade().unwrap(), BuildSteps::ARCH_EVAL) { //TODO check ARCH ?
                                    let mut builder = PythonArchEval::new(file_symbol.upgrade().unwrap());
                                    builder.eval_arch(session);
                                }
                            },
                            None => {}
                        }
                    }
                    let mut next_sym_refs = Symbol::next_refs(session, &sym, &mut None, &mut vec![]);
                    index += 1;
                    if next_sym_refs.len() >= 1 {
                        results.pop_front();
                        index -= 1;
                        for next_results in next_sym_refs {
                            results.push_back(next_results);
                        }
                    }
                },
                _ => {
                    index += 1;
                }
            }
        }
        return Vec::from(results) // :'( a whole copy?
    }

    pub fn all_symbols(&self) -> impl Iterator<Item= Rc<RefCell<Symbol>>> + '_ {
        //return an iterator on all symbols of self. only symbols in symbols and module_symbols will
        //be returned.
        let mut iter: Vec<Box<dyn Iterator<Item = Rc<RefCell<Symbol>>>>> = Vec::new();
        match self {
            Symbol::File(f) => {
                iter.push(Box::new(self.iter_symbols().flat_map(|(name, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone()))));
            },
            Symbol::Class(c) => {
                iter.push(Box::new(self.iter_symbols().flat_map(|(name, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone()))));
            },
            Symbol::Package(PackageSymbol::Module(m)) => {
                iter.push(Box::new(self.iter_symbols().flat_map(|(name, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone()))));
                iter.push(Box::new(m.module_symbols.values().map(|x| x.clone())));
            },
            Symbol::Package(PackageSymbol::PythonPackage(p)) => {
                iter.push(Box::new(self.iter_symbols().flat_map(|(name, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone()))));
                iter.push(Box::new(p.module_symbols.values().map(|x| x.clone())));
            },
            Symbol::Namespace(n) => {
                iter.push(Box::new(n.directories.iter().flat_map(|x| x.module_symbols.values().map(|x| x.clone()))));
            },
            _ => {}
        }
        iter.into_iter().flatten()
    }

    /* return the Symbol (class, function or file) the closest to the given offset */
    pub fn get_scope_symbol(file_symbol: Rc<RefCell<Symbol>>, offset: u32) -> Rc<RefCell<Symbol>> {
        let mut result = file_symbol.clone();
        let section_id = file_symbol.borrow().as_symbol_mgr().get_section_for(offset);
        for (sym_name, sym_map) in file_symbol.borrow().iter_symbols() {
            match sym_map.get(&section_id.index) {
                Some(symbols) => {
                    for symbol in symbols.iter() {
                        let typ = symbol.borrow().typ().clone();
                        match typ {
                            SymType::CLASS => {
                                if symbol.borrow().range().start().to_u32() < offset && symbol.borrow().range().end().to_u32() > offset {
                                    result = Symbol::get_scope_symbol(symbol.clone(), offset);
                                }
                            },
                            SymType::FUNCTION => {
                                if symbol.borrow().range().start().to_u32() < offset && symbol.borrow().range().end().to_u32() > offset {
                                    result = Symbol::get_scope_symbol(symbol.clone(), offset);
                                }
                            }
                            _ => {}
                        }
                    }
                },
                None => {}
            }
        }
        return result
    }

    //infer a name, given a position
    pub fn infer_name(odoo: &mut SyncOdoo, on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: Option<u32>) -> Vec<Rc<RefCell<Symbol>>> {
        let mut results: Vec<Rc<RefCell<Symbol>>> = vec![];
        //TODO implement 'super' behaviour in hooks
        let on_symbol = on_symbol.borrow();
        results = on_symbol.get_content_symbol(name, position.unwrap_or(u32::MAX));
        if results.len() == 0 && !vec![SymType::FILE, SymType::PACKAGE, SymType::ROOT].contains(&on_symbol.typ()) {
            let parent = on_symbol.parent().as_ref().unwrap().upgrade().unwrap();
            return Symbol::infer_name(odoo, &parent, name, position);
        }
        if results.len() == 0 && (on_symbol.name() != "builtins" || on_symbol.typ() != SymType::FILE) {
            let builtins = odoo.get_symbol(&(vec![S!("builtins")], vec![]), u32::MAX)[0].clone();
            return Symbol::infer_name(odoo, &builtins, name, None);
        }
        results
    }

    pub fn get_sorted_symbols(&self) -> impl Iterator<Item = Rc<RefCell<Symbol>>> {
        let mut symbols: Vec<Rc<RefCell<Symbol>>> = Vec::new();
        match self {
            Symbol::Class(_) | Symbol::Function(_) | Symbol::File(_) | Symbol::Package(PackageSymbol::Module(_)) |
            Symbol::Package(PackageSymbol::PythonPackage(_)) => {
                let syms = self.iter_symbols();
                for (sym_name, map) in syms {
                    for (index, syms) in map.iter() {
                        for sym in syms.iter() {
                            symbols.push(sym.clone());
                        }
                    }
                }
            },
            _ => {panic!()}
        }
        symbols.sort_by_key(|s| s.borrow().range().start().to_u32());
        symbols.into_iter()
    }

    /* similar to get_symbol: will return the symbol that is under this one with the specified name.
    However, if the symbol is a class or a model, it will search in the base class or in comodel classes
    if not all, it will return the first found. If all, the all found symbols are returned, but the first one
    is the one that is overriding others.
    :param: from_module: optional, can change the from_module of the given class */
    pub fn get_member_symbol(&self, session: &mut SessionInfo, name: &String, from_module: Option<Rc<RefCell<Symbol>>>, prevent_comodel: bool, all: bool, diagnostics: &mut Vec<Diagnostic>) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result: Vec<Rc<RefCell<Symbol>>> = vec![];
        let mod_sym = self.get_module_symbol(name);
        if let Some(mod_sym) = mod_sym {
            if all {
                result.push(mod_sym);
            } else {
                return vec![mod_sym];
            }
        }
        let content_sym = self.get_content_symbol(name, u32::MAX);
        if content_sym.len() >= 1 {
            if all {
                result.extend(content_sym);
            } else {
                return content_sym;
            }
        }
        if self.typ() == SymType::CLASS && self.as_class_sym()._model.is_some() && !prevent_comodel {
            let model = session.sync_odoo.models.get(&self.as_class_sym()._model.as_ref().unwrap().name);
            if let Some(model) = model {
                let loc_symbols = model.clone().borrow().get_symbols(session, from_module.clone().unwrap_or(self.find_module().expect("unable to find module")));
                for loc_sym in loc_symbols {
                    if self.is_equal(&loc_sym) {
                        continue;
                    }
                    let attribut = loc_sym.borrow().get_member_symbol(session, name, None, true, all, diagnostics);
                    if all {
                        result.extend(attribut);
                    } else {
                        return attribut;
                    }
                }
            }
        }
        if !all && result.len() != 0 {
            return result;
        }
        if self.typ() == SymType::CLASS {
            for base in self.as_class_sym().bases.iter() {
                let s = base.borrow().get_member_symbol(session, name, from_module.clone(), prevent_comodel, all, diagnostics);
                if s.len() != 0 {
                    if all {
                        result.extend(s);
                    } else {
                        return s;
                    }
                }
            }
        }
        result
    }

    pub fn is_equal(&self, other: &Rc<RefCell<Symbol>>) -> bool {
        return Weak::ptr_eq(&self.weak_self().unwrap_or(Weak::new()), &Rc::downgrade(other));
    }

    pub fn print_dependencies(&self) {
        println!("------- Output dependencies of {} -------", self.name());
        println!("--- ARCH");
        println!("--- on ARCH");
        for sym in self.dependencies()[0][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- ARCH EVAL");
        println!("--- on ARCH");
        for sym in self.dependencies()[1][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- ODOO");
        println!("--- on ARCH");
        for sym in self.dependencies()[2][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ARCH EVAL");
        for sym in self.dependencies()[2][1].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ODOO");
        for sym in self.dependencies()[2][2].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- VALIDATION");
        println!("--- on ARCH");
        for sym in self.dependencies()[3][0].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ARCH EVAL");
        for sym in self.dependencies()[3][1].iter() {
            println!("{:?}", sym.borrow().paths());
        }
        println!("--- on ODOO");
        for sym in self.dependencies()[3][2].iter() {
            println!("{:?}", sym.borrow().paths());
        }
    }

    /*fn _debug_print_graph_node(&self, acc: &mut String, level: u32) {
        for _ in 0..level {
            acc.push_str(" ");
        }
        acc.push_str(format!("{:?} {:?}\n", self.typ(), self.name()).as_str());
        for section in self.localized_sym.iter() {
            for local in section.iter() {
                for _ in 0..level {
                    acc.push_str(" ");
                }
                acc.push_str(format!("at {}", local.borrow().range.start().to_u32()).as_str());
            }
        }
        if let Some(symbol_location) = &self.symbols {
            if symbol_location.symbols().len() > 0 {
                for _ in 0..level {
                    acc.push_str(" ");
                }
                acc.push_str("SYMBOLS:\n");
                for sym in symbol_location.symbols().values() {
                    sym.borrow()._debug_print_graph_node(acc, level + 1);
                }
            }
        }
        if self.module_symbols.len() > 0 {
            for _ in 0..level {
                acc.push_str(" ");
            }
            acc.push_str("MODULES:\n");
            for (_, module) in self.module_symbols.iter() {
                module.borrow_mut()._debug_print_graph_node(acc, level + 1);
            }
        }
    }

    pub fn debug_to_json(&self) -> Value {
        let mut modules = vec![];
        let mut symbols = vec![];
        let mut offsets = vec![];
        for section in self.localized_sym.iter() {
            for local in section.iter() {
                offsets.push(local.borrow().range.start().to_u32());
            }
        }
        for s in self.module_symbols.values() {
            modules.push(s.borrow_mut().debug_to_json());
        }
        for s in self.symbols.as_ref().unwrap().symbols().values() {
            symbols.push(s.borrow_mut().debug_to_json());
        }
        json!({
            "name": self.name,
            "type": self.sym_type.to_string(),
            "offsets": offsets,
            "module_symbols": modules,
            "symbols": symbols,
        })
    }

    pub fn debug_print_graph(&self) -> String {
        info!("----Starting output of symbol debug display----");
        let mut res: String = String::new();
        self._debug_print_graph_node(&mut res, 0);
        info!("----End output of symbol debug display----");
        res
    }*/
}