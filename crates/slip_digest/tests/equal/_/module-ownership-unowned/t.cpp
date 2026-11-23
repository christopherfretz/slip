// std: c++20
// digest-kinds rule ModuleOwnership `Unowned` is every declaration Kiln makes (no other value is set, kiln_ast/src/decl.rs:512).
export module m;
#ifdef B
int x;
#else
int x;
#endif
