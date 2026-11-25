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
// a friend declaration naming a member through a template-id with an operand
template <int N> struct Q { static int f(); };
struct F { friend int Q<sizeof(make(x))>::f(); };
