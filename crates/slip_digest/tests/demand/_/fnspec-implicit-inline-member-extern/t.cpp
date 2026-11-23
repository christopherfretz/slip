#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 1): a member of an explicit
// instantiation declaration whose pattern is defined `inline` out of
// line: the use instantiates the definition and marks it implicitly
// inline.
template <class T> struct X { int g(int a); };
template <class T> inline int X<T>::g(int a) { return a; }
extern template struct X<int>;
X<int> x;
#ifdef B
int use() { return x.g(1); }
#endif
