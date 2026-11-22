#include "side.h"
// §5.7 `lexical_ctx` (WP-033 review, item 9): a member of a class template defined outside it is defined in an instantiation on use; its row does not move with the use.
template <class T> struct A { void g(); };
template <class T> void A<T>::g() {}
A<int> a;
#ifdef B
void y() { a.g(); }
#endif
