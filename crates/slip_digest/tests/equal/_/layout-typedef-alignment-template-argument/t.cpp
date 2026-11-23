#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the typedef as a class template's argument is read canonical by Kiln's layout (`layout_desugar_once`, kiln_ast `layout_sugar.rs:34`): no layout and no unit changes.
typedef int I __attribute__((aligned(8)));
#ifdef B
template <class T> struct X { char c; T t; };
X<I> x;
#else
template <class T> struct X { char c; T t; };
X<int> x;
#endif
