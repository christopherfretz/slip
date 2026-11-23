#include "side.h"
// digest §14 (WP-045 review probe): a member enumeration defined out of
// line.
template <class T> struct X { enum class E : int; };
template <class T> enum class X<T>::E : int { e0, e1 };
X<int> x;
#ifdef B
int use() { return (int)X<int>::E::e1; }
#endif
