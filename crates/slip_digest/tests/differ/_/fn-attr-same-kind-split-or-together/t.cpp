#include "side.h"
// §10.1 (WP-100): a function's nodes are kept per declaration. Two `patchable_function_entry` on the first declaration against one on it and one on the definition: one list over the chain would be the same, but Kiln reads the first node of the latest declaration that has one (`fn_attr`, kiln_codegen new_delete.rs:169), and clang 19 emits "patchable-function-entry"="1" against "2".
#ifdef B
__attribute__((patchable_function_entry(1))) __attribute__((patchable_function_entry(2))) void f();
void f() {}
#else
__attribute__((patchable_function_entry(1))) void f();
__attribute__((patchable_function_entry(2))) void f() {}
#endif
