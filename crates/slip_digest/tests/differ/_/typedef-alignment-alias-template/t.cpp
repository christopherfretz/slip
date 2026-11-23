#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an alias template keeps its argument's typedef for layout in Kiln (clang 22; clang 19 makes it canonical, `kiln_ast/src/layout_sugar.rs:47`).
typedef int I __attribute__((aligned(8)));
template <class T> using A = T;
#ifdef B
struct S { char c; A<I> i; };
#else
struct S { char c; A<int> i; };
#endif
