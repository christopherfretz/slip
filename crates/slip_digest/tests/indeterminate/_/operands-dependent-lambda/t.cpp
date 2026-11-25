#include "side.h"
// WP-156 fix round 2 (X4-X6, the walk of a dependent operand): a lambda
// inside an instantiation-dependent operand in a pattern's body and in a
// class template's member. Its body's declarations and statements are not
// walked for holders, so the operand unit is indeterminate
// (`operands/dependent-body`), never equal: here its body names `make(x)`
// through `W2<sizeof(make(x))>`. (In a pattern's signature the lambda
// makes the function itself indeterminate already.)
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <int N> struct W2 { int f() { return N; } };
template <class T> int fl(T t) {
  decltype(t, [] { return W2<sizeof(make(x))>(); }(), 0) l = 0;
  return l;
}
template <class T> struct SL { decltype(T(), [] { return W2<sizeof(make(x))>(); }(), 0) m; };
