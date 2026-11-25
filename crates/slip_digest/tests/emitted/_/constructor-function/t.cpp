#include "side.h"
// WP-155: a function with `constructor` or `destructor` is emitted and
// called before (after) `main`; so is an instantiation of a member
// template that has it (`zz153-f1`): its `definition` unit is marked.
extern "C" int puts(const char *);
__attribute__((constructor)) void init() { puts("init"); }
__attribute__((destructor)) void fini() { puts("fini"); }
struct S {
  template <class U = int> __attribute__((constructor)) static auto s() { puts("ctor-fn ran"); return 1; }
};
#ifndef B
// The deduced return type instantiates the definition.
int x = sizeof(S::s());
#endif
