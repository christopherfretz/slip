#include "side.h"
// WP-156 fix round 2 (X2): a destructor named by member access through a
// template-id with an operand. Kiln resolves the name to the destructor
// and keeps no type as written: a member access naming a destructor of a
// class that such a template-id or `decltype` names in the translation
// unit is indeterminate in its operand unit (`kiln/destructor-name`).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
Wr<int> wv;
void dtor() { wv.~Wr<decltype(make(x))>(); }
