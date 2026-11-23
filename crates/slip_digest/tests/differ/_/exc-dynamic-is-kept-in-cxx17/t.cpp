// std: c++17
// args: -Wno-dynamic-exception-spec
#include "side.h"
// digest-kinds §4.2 rule 3: a dynamic exception specification of a function's own is written as written in C++17 too. It is canonically `None` there (kiln_ast/src/types/canon.rs:657), but Kiln keeps its types and checks an overrider's against them (kiln_sema/src/class/virtual/mod.rs `check_override_exc_spec`); clang 19 also emits its filter. Was `equal/_/exc-dynamic-is-none-in-cxx17`.
#ifdef B
void f() throw(int);
#else
void f() noexcept(false);
#endif
