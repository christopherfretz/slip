#include "side.h"
// WP-045 decision 3: a specialisation whose kind is
// ExplicitInstantiationDefinition is not of the definition class: its
// definition is made at the explicit instantiation, used before it or
// not.
template <class T> T f(T x) { return x; }
template <class T> struct C { int m; int g() { return m; } };
#ifdef B
int use() { C<int> c{}; return f(1) + c.g(); }
#endif
template int f<int>(int);
template struct C<int>;
