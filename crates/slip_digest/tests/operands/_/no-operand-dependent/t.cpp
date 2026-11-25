#include "side.h"
// WP-156 item 3: an instantiation-dependent operand in a pattern is not
// written (the pattern's own spellings hold it); the pattern is never
// instantiated, so no unit has an operand unit.
int make(int);
template <class T> decltype(make(T())) f(T);
template <class T> struct S { decltype(make(T())) m; };
#ifdef B
template <class T> void g(decltype(make(T())) *);
#else
template <class T> void g(decltype(make(T())) *);
#endif
