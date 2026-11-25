#include "side.h"
// WP-156 fix round 2 (X5): a holder Kiln keeps is read; side B loses
// lib::make(lib::X), so the operand binds the template and the unit differs.
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
template <class T, class U> concept Same = __is_same(T, U);
// same, in a class template's field (pattern only)
template <class T> struct FD { W2<sizeof(T) + sizeof(Wr<decltype(make(x))>)> f; };
