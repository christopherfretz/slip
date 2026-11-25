#include "side.h"
// WP-156 fix round 2 (X8): `_BitInt(e)`. Kiln keeps the width as a number
// (or a profile in a pattern), never the expression: a declaration whose
// written type has a `_BitInt` is indeterminate in its operand unit
// (`kiln/bit-int-width`) where an expression that is no literal is between
// the bounds around its name (`lit` fails nothing).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };
_BitInt(sizeof(make(x)) * 8) bi = 0;
unsigned _BitInt(sizeof(make(x)) * 8) bu = 0;
template <class T> _BitInt(sizeof(make(x)) * 8) bt(T) { return 0; }
_BitInt(32) lit = 0;
