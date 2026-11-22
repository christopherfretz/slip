#include "side.h"
// WP-034 (§6): a range-for over a class template specialisation completed before the body on B (in a function: a variable would change the order unit).
template <class T> struct R { T *begin(); T *end(); };
#ifdef B
void g() { R<int> x; (void)x; }
#endif
void f(R<int> &r) { for (int e : r) ; }
