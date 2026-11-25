#include "side.h"
// WP-156 fix round 2 (X7): a holder Kiln keeps is read; side B loses
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
// a friend naming a function template specialisation with explicit arguments
template <int N> void ftf() {}
struct FF { friend void ftf<sizeof(make(x))>(); };
