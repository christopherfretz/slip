#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the typedef as a class template's argument: the specialization is `X<int>` and its layout reads the canonical argument (`layout_desugar_once`, kiln_ast `layout_sugar.rs:34`; clang 19 agrees).
typedef int I __attribute__((aligned(8)));
template <class T> struct X { char c; T t; };
#ifdef B
X<I> x;
#else
X<int> x;
#endif
static_assert(alignof(X<I>) == 4 && sizeof(X<I>) == 8);
