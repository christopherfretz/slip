#include "side.h"
// WP-034 (§6): a range-for over a class template specialisation completed before the body on B.
template <class T> struct R { T *begin(); T *end(); };
#ifdef B
R<int> x;
#endif
void f(R<int> &r) { for (int e : r) ; }
