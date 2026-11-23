#include "side.h"
// A constructor defined in a class template: Kiln keeps its mem-initializers for instantiation only (`Sema::act_on_mem_initializers`, `pattern_mem_inits`), not in the AST, so a changed initializer is not seen (spec/landed/digest-kinds/WP-041.md, For Kiln 10).
template<int N> struct Bq { Bq(); Bq(int); };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { int m;
#ifdef B
  S() : m(2) { }
#else
  S() : m(1) { }
#endif
};
