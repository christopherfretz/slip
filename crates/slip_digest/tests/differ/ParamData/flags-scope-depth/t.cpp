#include "side.h"
// §5.4 ParamData `flags-scope-depth`: a lambda's parameter in a default
// argument is one prototype scope deeper than f's (Kiln
// `kiln_sema/src/decl.rs:642-644`). A parameter of a parameter's function
// type (`void (*g)(int b)`) is in no stream: no function adopts it.
#ifdef B
void f(int a = [](int b) { return b; }(1));
#else
void f(int a = [](int) { return 1; }(1));
#endif
