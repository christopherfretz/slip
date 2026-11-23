#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an alias template keeps its argument's typedef for layout (kiln_ast `layout_sugar.rs:47`).
typedef int I __attribute__((aligned(8)));
template <class T> using A = T;
struct S { char c; A<I> i; };
