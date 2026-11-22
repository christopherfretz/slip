#include "side.h"
// §5.7 `lexical_ctx` (WP-033 review, item 9): a static data member of a class template defined outside it is defined in an instantiation on use; its row does not move with the use.
template <class T> struct A { static int s; };
template <class T> int A<T>::s = 1;
A<int> a;
#ifdef B
int y() { return A<int>::s; }
#endif
