#include "side.h"
// WP-156 fix round 2 (X1): a holder Kiln keeps is read; side B loses
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
// same, the class named by type arguments holding an operand
template <class T> struct Q2 { static int f(); };
struct F2 { friend int Q2<Wr<decltype(make(x))>>::f(); };
