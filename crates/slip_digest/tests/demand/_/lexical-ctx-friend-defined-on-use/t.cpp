#include "side.h"
// §5.7 `lexical_ctx` (WP-033 review, item 9): a friend defined in a class template is defined in an instantiation on use; its row does not move with the use.
template <class T> struct A { friend void f(A) {} };
A<int> a;
#ifdef B
void y() { f(a); }
#endif
